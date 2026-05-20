use std::{
    collections::HashMap,
    f64::consts::PI,
    fs::File,
    io::BufWriter,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use anyhow::Error;
use chrono::{
    DateTime,
    Utc,
};
use numeris::{
    Vector3,
    vector,
};
use parking_lot::Mutex;
use satkit::{
    ITRFCoord,
    sgp4::{
        SGP4State,
        sgp4,
    },
};
use serde::Serialize;

use crate::{
    geo::Geodetic,
    satnogs::{
        self,
        NoradCatId,
        SatnogsApi,
    },
};

#[derive(Debug)]
pub struct Satellites {
    data_dir: PathBuf,
    satnogs_api: SatnogsApi,

    satellites: Vec<Option<Satellite>>,
    free_list: Vec<SatelliteHandle>,
    by_satnogs_id: HashMap<satnogs::SatelliteId, SatelliteHandle>,
}

impl Satellites {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let data_dir = data_dir.as_ref();

        if !data_dir.exists() {
            tracing::debug!(?data_dir, "Creating directory for satellite data");
            std::fs::create_dir_all(&data_dir)?;
        }

        let satnogs_api = SatnogsApi::new();

        Ok(Self {
            data_dir: data_dir.to_owned(),
            satnogs_api,
            satellites: vec![],
            free_list: vec![],
            by_satnogs_id: HashMap::new(),
        })
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        let satellites = self.satnogs_api.satellites().await?;
        let tle = self.satnogs_api.tle().await?;
        let transmitters = self.satnogs_api.transmitters().await?;

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

            if let Some(&handle) = self.by_satnogs_id.get(&satellite.sat_id) {
                *self
                    .get_mut(handle)
                    .expect("invalid handle from satnogs id") =
                    Satellite::new(handle, satellite, tle, transmitters);
            }
            else {
                self.insert(|handle| Satellite::new(handle, satellite, tle, transmitters));
            }
        }

        Ok(())
    }

    fn write_data_file<T>(&self, file_name: impl AsRef<Path>, value: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        let writer = BufWriter::new(File::create(self.data_dir.join(file_name))?);
        serde_json::to_writer_pretty(writer, value)?;
        Ok(())
    }

    fn insert(&mut self, make_satellite: impl FnOnce(SatelliteHandle) -> Satellite) {
        if let Some(handle) = self.free_list.pop() {
            assert!(self.satellites[handle.index].is_none());
            self.satellites[handle.index] = Some(make_satellite(handle));
        }
        else {
            let handle = SatelliteHandle {
                generation: 1,
                index: self.satellites.len(),
            };
            self.satellites.push(Some(make_satellite(handle)));
        }
    }

    pub fn get(&self, handle: SatelliteHandle) -> Option<&Satellite> {
        let satellite = self.satellites.get(handle.index)?.as_ref()?;
        assert_eq!(satellite.handle.index, handle.index);
        (satellite.handle.generation == handle.generation).then_some(satellite)
    }

    fn get_mut(&mut self, handle: SatelliteHandle) -> Option<&mut Satellite> {
        let satellite = self.satellites.get_mut(handle.index)?.as_mut()?;
        assert_eq!(satellite.handle.index, handle.index);
        (satellite.handle.generation == handle.generation).then_some(satellite)
    }
}

#[derive(Clone, Debug)]
pub struct Satellite {
    handle: SatelliteHandle,

    satnogs_satellite: satnogs::Satellite,
    satnogs_tle: Option<satnogs::Tle>,
    satnogs_transmitters: Vec<satnogs::Transmitter>,

    // note: this is in a Mutex, because satkit caches some data in it.
    //
    // todo: remove mutex. let caller pass a &mut to a workspace struct to get_location in which we
    // store that instead. we will likely have to implement our own SGP4Source
    satkit_tle: Option<Arc<Mutex<satkit::TLE>>>,
}

impl Satellite {
    fn new(
        handle: SatelliteHandle,
        satnogs_satellite: satnogs::Satellite,
        satnogs_tle: Option<satnogs::Tle>,
        satnogs_transmitters: Vec<satnogs::Transmitter>,
    ) -> Self {
        let mut satkit_tle = None;

        if let Some(tle) = &satnogs_tle {
            match satkit::TLE::load_3line(&tle.tle0, &tle.tle1, &tle.tle2) {
                Ok(tle) => {
                    satkit_tle = Some(Arc::new(Mutex::new(tle)));
                }
                Err(error) => {
                    tracing::error!("Can't parse TLE: {error}");
                }
            }
        }

        Self {
            handle,
            satnogs_satellite,
            satnogs_tle,
            satnogs_transmitters,
            satkit_tle,
        }
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

    pub fn get_location(&self, times: &[DateTime<Utc>]) -> Option<Vec<SatelliteState>> {
        if let Some(tle) = &self.satkit_tle {
            let mut source = tle.lock();

            match sgp4(&mut *source, times) {
                Ok(state) => {
                    Some(
                        times
                            .into_iter()
                            .copied()
                            .enumerate()
                            .map(|(index, time)| {
                                SatelliteState::from_sgp4_solution(time, index, &state)
                            })
                            .collect(),
                    )
                }
                Err(error) => {
                    tracing::error!("SGP4 propagation failed: {error}");
                    None
                }
            }
        }
        else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SatelliteHandle {
    generation: usize,
    index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct SatelliteState {
    time: DateTime<Utc>,

    /// Satellite position in the International Terrestrial Reference Frame
    /// (ITRF)
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
        // transform ITRF -> ENU
        let transform = reference.position.q_enu2itrf().conjugate();

        RelativeState {
            position: transform * (self.position.itrf - reference.position.itrf),
            velocity: transform * (self.velocity - reference.velocity),
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
}

impl ReferenceState {
    pub fn from_geodetic(geodetic: Geodetic) -> Self {
        Self {
            position: ITRFCoord::from_geodetic_deg(
                geodetic.latitude,
                geodetic.longitude,
                geodetic.altitude,
            ),
            velocity: Vector3::zeros(),
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
    pub fn distance(&self) -> f64 {
        self.position.norm()
    }

    pub fn radial_speed(&self) -> f64 {
        self.position.normalize().dot(&self.velocity)
    }

    pub fn free_space_path_loss(&self, frequency: f64) -> f64 {
        free_space_path_loss(self.distance(), frequency)
    }

    pub fn doppler_shift(&self, frequency: f64) -> f64 {
        doppler_shift(self.radial_speed(), frequency)
    }
}

pub fn free_space_path_loss(distance: f64, frequency: f64) -> f64 {
    // should this be in dB?

    (4.0 * PI * distance * frequency * satkit::consts::C).powi(2)
}

pub fn doppler_shift(radial_speed: f64, frequency: f64) -> f64 {
    radial_speed * frequency / satkit::consts::C
}
