use std::{
    collections::HashMap,
    f64::consts::PI,
    fmt::Debug,
    fs::File,
    io::{
        BufReader,
        BufWriter,
    },
    ops::{
        Deref,
        DerefMut,
    },
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Error,
    anyhow,
};
use chrono::{
    DateTime,
    Utc,
};
use numeris::{
    Quaternion,
    Vector3,
    vector,
};
use satkit::{
    ITRFCoord,
    sgp4::{
        SGP4Source,
        SGP4State,
        sgp4,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    geo::Geodetic,
    satnogs::{
        self,
        NoradCatId,
        SatnogsApi,
    },
};

/// An in-memory satellite database
#[derive(Debug, Default)]
pub struct Satellites {
    satellites: Vec<Option<Satellite>>,
    free_list: Vec<SatelliteHandle>,
    by_satnogs_id: HashMap<satnogs::SatelliteId, SatelliteHandle>,
}

impl Satellites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates database from Satnogs API
    ///
    /// This will fetch satellites, TLEs and transmitters from the Satnogs API
    /// and insert them into this DB, replacing any existing satellites. Handles
    /// to existing satellites stay intact.
    ///
    /// This does not clear the DB before inserting new satellites, so anything
    /// removed from the Satnogs DB will be kept.
    pub async fn update_from_satnogs(&mut self, satnogs_api: &SatnogsApi) -> Result<(), Error> {
        // we only use satnogs API atm, but this is what gpredict uses:
        // https://github.com/csete/gpredict/blob/73937f7e9825747d8482884d25c65904b5ff0ef5/src/sat-cfg.c#L228-L248

        let satellites = satnogs_api.satellites().await?;
        let tle = satnogs_api.tle().await?;
        let transmitters = satnogs_api.transmitters().await?;

        let mut tle_index = HashMap::with_capacity(satellites.len());
        for tle in tle {
            tle_index.insert(tle.sat_id.clone(), tle);
        }

        let mut transmitter_index = HashMap::with_capacity(satellites.len());
        for transmitter in transmitters {
            transmitter_index
                .entry(transmitter.sat_id.clone())
                .or_insert(vec![])
                .push(transmitter);
        }

        for satellite in satellites {
            let tle = tle_index.remove(&satellite.sat_id);
            let transmitters = transmitter_index
                .remove(&satellite.sat_id)
                .unwrap_or_default();

            self.insert_or_replace_from_satnogs(satellite, tle, transmitters, true);
        }

        Ok(())
    }

    fn insert_or_replace_from_satnogs(
        &mut self,
        satellite: satnogs::Satellite,
        tle: Option<satnogs::Tle>,
        transmitters: Vec<satnogs::Transmitter>,
        replace_existing: bool,
    ) {
        if let Some(&handle) = self.by_satnogs_id.get(&satellite.sat_id) {
            if replace_existing {
                *self
                    .get_mut(handle)
                    .expect("invalid handle from satnogs id") =
                    Satellite::new(handle, satellite, tle, transmitters);
            }
        }
        else {
            self.insert(|handle| Satellite::new(handle, satellite, tle, transmitters));
        }
    }

    pub fn write_to_disk(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        tracing::debug!(path = ?path.as_ref(), "writing satellite database to file");

        let serialized = self
            .satellites
            .iter()
            .filter_map(|satellite| {
                satellite.as_ref().map(|satellite| {
                    SerializedSatellite {
                        satellite: satellite.satnogs_satellite.clone(),
                        tle: satellite.satnogs_tle.clone(),
                        transmitters: satellite.satnogs_transmitters.clone(),
                    }
                })
            })
            .collect::<Vec<_>>();

        let writer = BufWriter::new(File::create(path)?);
        serde_json::to_writer_pretty(writer, &serialized)?;

        Ok(())
    }

    pub fn read_from_disk(
        &mut self,
        path: impl AsRef<Path>,
        replace_existing: bool,
    ) -> Result<(), Error> {
        tracing::debug!(path = ?path.as_ref(), "reading satellite database from file");

        let reader = BufReader::new(File::open(path)?);
        let serialized: Vec<SerializedSatellite> = serde_json::from_reader(reader)?;

        self.clear();

        for satellite in serialized {
            self.insert_or_replace_from_satnogs(
                satellite.satellite,
                satellite.tle,
                satellite.transmitters,
                replace_existing,
            );
        }

        Ok(())
    }

    pub fn clear(&mut self) {
        for satellite in self.satellites.drain(..) {
            if let Some(satellite) = satellite {
                self.free_list.push(satellite.handle);
            }
        }
    }

    pub fn insert(
        &mut self,
        make_satellite: impl FnOnce(SatelliteHandle) -> Satellite,
    ) -> SatelliteHandle {
        if let Some(mut handle) = self.free_list.pop() {
            handle.generation += 1;
            assert!(self.satellites[handle.index].is_none());

            let satellite = make_satellite(handle);

            self.by_satnogs_id
                .insert(satellite.satnogs_satellite.sat_id.clone(), handle);
            self.satellites[handle.index] = Some(satellite);

            handle
        }
        else {
            let handle = SatelliteHandle {
                generation: 1,
                index: self.satellites.len(),
            };

            let satellite = make_satellite(handle);

            self.by_satnogs_id
                .insert(satellite.satnogs_satellite.sat_id.clone(), handle);
            self.satellites.push(Some(satellite));

            handle
        }
    }

    pub fn get(&self, handle: SatelliteHandle) -> Option<&Satellite> {
        let satellite = self.satellites.get(handle.index)?.as_ref()?;
        assert_eq!(satellite.handle.index, handle.index);
        (satellite.handle.generation == handle.generation).then_some(satellite)
    }

    pub fn get_mut(&mut self, handle: SatelliteHandle) -> Option<&mut Satellite> {
        let satellite = self.satellites.get_mut(handle.index)?.as_mut()?;
        assert_eq!(satellite.handle.index, handle.index);
        (satellite.handle.generation == handle.generation).then_some(satellite)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Satellite> {
        self.satellites
            .iter()
            .filter_map(|satellite| satellite.as_ref())
    }

    pub fn get_by_id(&self, id: &satnogs::SatelliteId) -> Option<SatelliteHandle> {
        self.by_satnogs_id.get(id).copied()
    }
}

/// A file-backed satellite database.
///
/// This wraps [`Satellites`], but reads and writes the satellites to file.
#[derive(Debug)]
pub struct SatelliteDatabase {
    satellites: Satellites,
    satnogs_api: SatnogsApi,
    path: PathBuf,
}

impl SatelliteDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let mut satellites = Satellites::default();
        if path.exists() {
            satellites.read_from_disk(path, true)?;
        }

        let satnogs_api = SatnogsApi::new();

        Ok(Self {
            path: path.to_owned(),
            satnogs_api,
            satellites,
        })
    }

    pub fn write_to_disk(&self) -> Result<(), Error> {
        self.satellites.write_to_disk(&self.path)
    }

    pub fn read_from_disk(&mut self, replace_existing: bool) -> Result<(), Error> {
        self.satellites.read_from_disk(&self.path, replace_existing)
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        self.satellites
            .update_from_satnogs(&self.satnogs_api)
            .await?;
        self.write_to_disk()?;
        Ok(())
    }
}

impl Deref for SatelliteDatabase {
    type Target = Satellites;

    fn deref(&self) -> &Self::Target {
        &self.satellites
    }
}

impl DerefMut for SatelliteDatabase {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.satellites
    }
}

#[derive(Clone)]
pub struct Satellite {
    handle: SatelliteHandle,

    satnogs_satellite: satnogs::Satellite,
    satnogs_tle: Option<satnogs::Tle>,
    satnogs_transmitters: Vec<satnogs::Transmitter>,

    // note: this is in a Mutex, because satkit caches some data in it.
    //
    // todo: remove mutex. let caller pass a &mut to a workspace struct to get_location in which we
    // store that instead. we will likely have to implement our own SGP4Source
    satkit_tle: Option<satkit::TLE>,
}

impl Satellite {
    fn new(
        handle: SatelliteHandle,
        satnogs_satellite: satnogs::Satellite,
        satnogs_tle: Option<satnogs::Tle>,
        satnogs_transmitters: Vec<satnogs::Transmitter>,
    ) -> Self {
        let satkit_tle = satnogs_tle.as_ref().and_then(|tle| {
            satkit::TLE::load_3line(&tle.tle0, &tle.tle1, &tle.tle2)
                .inspect_err(|error| {
                    tracing::error!("Can't parse TLE: {error}");
                })
                .ok()
        });

        Self {
            handle,
            satnogs_satellite,
            satnogs_tle,
            satnogs_transmitters,
            satkit_tle,
        }
    }

    pub fn id(&self) -> &satnogs::SatelliteId {
        &self.satnogs_satellite.sat_id
    }

    pub fn handle(&self) -> SatelliteHandle {
        self.handle
    }

    pub fn name(&self) -> &str {
        &self.satnogs_satellite.name
    }

    pub fn norad_cat_id(&self) -> Option<NoradCatId> {
        self.satnogs_satellite.norad_cat_id
    }

    pub fn predict_state_into(
        &self,
        times: &[DateTime<Utc>],
        cache: &mut OrbitPropagationCache,
        output: &mut Vec<SatelliteState>,
    ) -> Result<(), Error> {
        tracing::debug!(satellite = ?self, num_times = times.len(), "predicting");

        if let Some(tle) = &self.satkit_tle {
            let mut source = CachedTle {
                handle: self.handle,
                tle,
                cache,
            };

            let state = sgp4(&mut source, times)?;

            output.reserve(times.len());
            for (index, time) in times.iter().copied().enumerate() {
                output.push(SatelliteState::from_sgp4_solution(time, index, &state));
            }

            Ok(())
        }
        else {
            Err(anyhow!("Satellite without TLE"))
        }
    }

    pub fn predict_state(
        &self,
        times: &[DateTime<Utc>],
        cache: &mut OrbitPropagationCache,
    ) -> Result<Vec<SatelliteState>, Error> {
        let mut output = vec![];
        self.predict_state_into(times, cache, &mut output)?;
        Ok(output)
    }

    pub fn is_alive(&self) -> bool {
        matches!(
            self.satnogs_satellite.status,
            satnogs::SatelliteStatus::Alive
        )
    }

    // todo: don't expose satnogs API
    pub fn transmitters(&self) -> &[satnogs::Transmitter] {
        &self.satnogs_transmitters
    }
}

impl Debug for Satellite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Satellite")
            .field("id", &self.satnogs_satellite.sat_id)
            .field("name", &self.satnogs_satellite.name)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SerializedSatellite {
    satellite: satnogs::Satellite,
    tle: Option<satnogs::Tle>,
    transmitters: Vec<satnogs::Transmitter>,
}

/// Caches orbit propagation data.
///
/// This only caches **one** satellite. You need to pass a seperate object for
/// every satellite.
#[derive(Clone, Debug, Default)]
pub struct OrbitPropagationCache {
    cached_satrec: Option<satkit::sgp4::SatRec>,
}

/// Helper that implements [`SGP4Source`]`.
///
/// satkit's sgp4 needs to cache some data. This is handled via
/// [`SGP4Source::satrec_mut`]. But we don't want our TLE, and with that our
/// satellites, to be mutable.
///
/// Instead we let the user of [`Satellite::get_state`] pass in a
/// [`OrbitPropagationCache`] and then pass this helper struct to satkit
/// instead. This helper struct will redirect the caching to our separate cache.
/// Thus the TLE doesn't need to be mutable.
#[derive(Debug)]
struct CachedTle<'a> {
    handle: SatelliteHandle,
    tle: &'a satkit::TLE,
    cache: &'a mut OrbitPropagationCache,
}

impl<'a> SGP4Source for CachedTle<'a> {
    fn epoch(&self) -> satkit::Instant {
        self.tle.epoch()
    }

    fn satrec_mut(&mut self) -> &mut Option<satkit::sgp4::SatRec> {
        &mut self.cache.cached_satrec
    }

    fn sgp4_init_args(&self) -> anyhow::Result<satkit::sgp4::SGP4InitArgs> {
        self.tle.sgp4_init_args()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SatelliteHandle {
    generation: usize,
    index: usize,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SatelliteState {
    time: DateTime<Utc>,

    /// Satellite position in the International Terrestrial Reference Frame
    /// (ITRF)
    #[serde(with = "serde_itrfcoord")]
    position: ITRFCoord,

    /// Satellite velocity in the International Terrestrial Reference Frame
    /// (ITRF)
    velocity: Vector3<f64>,
}

impl SatelliteState {
    fn from_itrf(time: DateTime<Utc>, position: Vector3<f64>, velocity: Vector3<f64>) -> Self {
        Self {
            time,
            position: ITRFCoord::from_vector(&position),
            velocity,
        }
    }

    fn from_sgp4_solution(time: DateTime<Utc>, index: usize, solution: &SGP4State) -> Self {
        let position = vector![
            solution.pos[(0, index)],
            solution.pos[(1, index)],
            solution.pos[(2, index)]
        ];
        let velocity = vector![
            solution.vel[(0, index)],
            solution.vel[(1, index)],
            solution.vel[(2, index)]
        ];

        let transform = satkit::frametransform::qteme2itrf(&time);

        SatelliteState::from_itrf(time, transform * position, transform * velocity)
    }

    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Returns geodetic latitude and longitude (in degrees)
    pub fn geodetic(&self) -> Geodetic {
        let (latitude, longitude, altitude) = self.position.to_geodetic_deg();
        Geodetic {
            latitude,
            longitude,
            altitude,
        }
    }

    pub fn relative(&self, reference: &ReferenceState) -> RelativeState {
        RelativeState {
            position: reference.q_itrf2enu * (self.position.itrf - reference.position.itrf),
            velocity: reference.q_itrf2enu * (self.velocity - reference.velocity),
        }
    }
}

/// A reference state
///
/// This is e.g. the position and velocity of the base station.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceState {
    /// Reference position in the International Terrestrial Reference Frame
    /// (ITRF)
    position: ITRFCoord,

    /// Reference velocity in the International Terrestrial Reference Frame
    /// (ITRF)
    velocity: Vector3<f64>,

    /// transform ITRF -> ENU
    q_itrf2enu: Quaternion<f64>,
}

impl ReferenceState {
    pub fn from_geodetic(geodetic: Geodetic) -> Self {
        let position =
            ITRFCoord::from_geodetic_deg(geodetic.latitude, geodetic.longitude, geodetic.altitude);

        Self {
            position,
            velocity: Vector3::zeros(),
            q_itrf2enu: position.q_enu2itrf().conjugate(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RelativeState {
    /// Relative position in ENU frame relative to reference in meters
    pub position: Vector3<f64>,

    /// Relative velocity in ENU frame relative to reference in m/s
    pub velocity: Vector3<f64>,
}

impl RelativeState {
    /// Azimuth in radians
    pub fn azimuth(&self) -> f64 {
        // todo: verify this is correct
        self.position.x().atan2(self.position.y())
    }

    /// Elevation in radians
    pub fn elevation(&self) -> f64 {
        let x = numeris::vector![self.position.x() + self.position.y()].norm();
        let y = self.position.z();
        (x / y).atan()
    }

    pub fn distance(&self) -> f64 {
        self.position.norm()
    }

    pub fn radial_speed(&self) -> f64 {
        self.position.normalize().dot(&self.velocity)
    }

    /// Free-space Path Loss in dB
    pub fn free_space_path_loss_db(&self, frequency: f64) -> f64 {
        free_space_path_loss_db(self.distance(), frequency)
    }

    /// Doppler shift in Hz
    pub fn doppler_shift(&self, frequency: f64) -> f64 {
        doppler_shift(self.radial_speed(), frequency)
    }
}

/// Free-space Path Loss in dB
pub fn free_space_path_loss_db(distance: f64, frequency: f64) -> f64 {
    20.0 * (distance.log10() + frequency.log10() + (4.0 * PI / satkit::consts::C))
}

/// Doppler shift in Hz
pub fn doppler_shift(radial_speed: f64, frequency: f64) -> f64 {
    radial_speed * frequency / satkit::consts::C
}

mod serde_itrfcoord {
    use numeris::Vector3;
    use satkit::ITRFCoord;
    use serde::{
        Deserialize,
        Deserializer,
        Serialize,
        Serializer,
    };

    pub fn serialize<S>(value: &ITRFCoord, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.itrf.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ITRFCoord, D::Error>
    where
        D: Deserializer<'de>,
    {
        let itrf: Vector3<f64> = Deserialize::deserialize(deserializer)?;
        Ok(ITRFCoord { itrf })
    }
}
