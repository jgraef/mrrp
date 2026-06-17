use std::{
    collections::{
        HashMap,
        VecDeque,
    },
    fs::File,
    io::{
        BufReader,
        BufWriter,
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
    TimeDelta,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    geo::Geodetic,
    satellite::{
        OrbitPropagationCache,
        ReferenceState,
        RelativeState,
        SatelliteHandle,
        SatelliteState,
        Satellites,
    },
    satnogs,
};

// todo: move
#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub low: u64,
    pub high: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackerOptions {
    pub look_back: TimeDelta,
    pub look_ahead: TimeDelta,
    pub time_resolution: TimeDelta,
    pub base_station: Option<Geodetic>,
    pub min_elevation: f64,
}

#[derive(Clone, Debug)]
pub struct Tracker {
    tracked: HashMap<SatelliteHandle, TrackedSatellite>,

    options: TrackerOptions,
    reference_state: Option<ReferenceState>,

    cache_file: Option<PathBuf>,
}

impl Tracker {
    pub fn new(options: TrackerOptions) -> Self {
        let reference_state = options
            .base_station
            .map(|base_station| ReferenceState::from_geodetic(base_station));

        Self {
            tracked: HashMap::new(),
            options,
            reference_state,
            cache_file: None,
        }
    }

    pub fn new_with_file_backed_cache(
        options: TrackerOptions,
        cache_file: impl AsRef<Path>,
        satellites: &Satellites,
    ) -> Result<Self, Error> {
        let mut this = Self::new(options);

        let cache_file = cache_file.as_ref();
        this.cache_file = Some(cache_file.to_owned());

        if cache_file.exists() {
            let reader = BufReader::new(File::open(cache_file)?);
            let data: Vec<FileCacheEntry> = serde_json::from_reader(reader)?;

            this.tracked.reserve(data.len());

            for entry in data {
                match TrackedSatellite::from_file_cache(entry, &satellites) {
                    Ok(tracked) => {
                        this.tracked.insert(tracked.handle, tracked);
                    }
                    Err(error) => {
                        tracing::error!(%error);
                    }
                }
            }
        }

        Ok(this)
    }

    pub fn update(&mut self, satellites: &mut Satellites) {
        self.update_with_time(satellites, Utc::now());
    }

    pub fn update_with_time(&mut self, satellites: &mut Satellites, now: DateTime<Utc>) {
        let look_ahead_until = now + self.options.look_ahead;
        let look_back_until = now - self.options.look_back;

        let mut times = vec![];
        let mut remove_later = vec![];
        let mut state_buffer = vec![];

        for (&handle, tracked) in self.tracked.iter_mut() {
            if let Some(satellite) = satellites.get(handle) {
                let last_known_state = tracked.latest_known_state();

                // check if we need to update
                if last_known_state
                    .as_ref()
                    .is_none_or(|(_, entry)| entry.state.time() < look_ahead_until)
                {
                    tracing::debug!(?satellite, "Updating satellite");

                    // time to start update with
                    let mut time = last_known_state.as_ref().map_or_else(
                        || now - self.options.look_back,
                        |(_, entry)| entry.state.time() + self.options.time_resolution,
                    );

                    // collect prediction times into buffer
                    assert!(times.is_empty());
                    while time <= look_ahead_until {
                        times.push(time);
                        time += self.options.time_resolution;
                    }

                    // do orbit propagation
                    assert!(state_buffer.is_empty());
                    if let Err(error) = satellite.predict_state_into(
                        &times,
                        &mut tracked.orbitprop_cache,
                        &mut state_buffer,
                    ) {
                        tracing::warn!(?satellite, %error, "Orbit propagation failed for satellite. Removing it from tracking");
                        remove_later.push(handle);
                    }

                    for state in state_buffer.drain(..) {
                        let relative_state = self
                            .reference_state
                            .as_ref()
                            .map(|reference_state| state.relative(reference_state));

                        let state_id = tracked.state_cache.push(state, relative_state);

                        if let Some(relative_state) = relative_state
                            && relative_state.elevation() > self.options.min_elevation
                        {
                            // check if this state belongs to the previous pass
                            if let Some(pass) = tracked.passes.back_mut()
                                && state_id.is_successor_of(&pass.last_state)
                            {
                                pass.push_state(state_id);
                            }
                            else {
                                tracked.passes.push_back(PassData {
                                    first_state: state_id,
                                    last_state: state_id,
                                });
                            }
                        }
                    }

                    // remove old state from cache
                    tracked.state_cache.remove_older_than(look_back_until);

                    // remove old passes
                    while let Some(pass) = tracked.passes.front() {
                        if tracked
                            .state_cache
                            .get(pass.last_state)
                            .is_some_and(|entry| entry.state.time() < look_back_until)
                        {
                            tracked
                                .passes
                                .pop_front()
                                .expect("peek_front returned Some, but pop_front None");
                        }
                    }

                    // clean up
                    times.clear();
                }
            }
            else {
                tracing::warn!(
                    ?handle,
                    "Tracked satellite not in database. Removing it from tracking."
                );
                remove_later.push(handle);
            }
        }

        // these need to be removed from tracking
        for handle in remove_later {
            self.tracked.remove(&handle);
        }
    }

    pub fn flush_cache_to_file(&self, satellites: &Satellites) -> Result<(), Error> {
        if let Some(cache_path) = &self.cache_file {
            tracing::debug!(?cache_path, "Flushing satellite tracking cache to file");

            let data = self
                .tracked
                .iter()
                .map(|(&handle, tracked)| {
                    satellites.get(handle).map(|satellite| {
                        FileCacheEntry {
                            satellite_id: satellite.id().clone(),
                            cache: tracked.state_cache.clone(),
                        }
                    })
                })
                .collect::<Vec<_>>();

            let writer = BufWriter::new(File::create(&cache_path)?);
            serde_json::to_writer_pretty(writer, &data)?;
        }

        Ok(())
    }

    pub fn track(&mut self, handle: SatelliteHandle) {
        self.tracked
            .entry(handle)
            .or_insert_with(|| TrackedSatellite::new(handle));
    }

    pub fn untrack(&mut self, handle: SatelliteHandle) {
        self.tracked.remove(&handle);
    }
}

#[derive(Clone, Debug)]
struct TrackedSatellite {
    handle: SatelliteHandle,

    state_cache: StateCache,

    /// Index for current state cache entry
    current_state: Option<StateCacheId>,

    orbitprop_cache: OrbitPropagationCache,

    passes: VecDeque<PassData>,
}

impl TrackedSatellite {
    fn new(handle: SatelliteHandle) -> Self {
        Self {
            handle,
            state_cache: StateCache::default(),
            current_state: None,
            orbitprop_cache: OrbitPropagationCache::default(),
            passes: VecDeque::new(),
        }
    }

    fn from_file_cache(
        cache_entry: FileCacheEntry,
        satellites: &Satellites,
    ) -> Result<Self, Error> {
        let handle = satellites
            .get_by_id(&cache_entry.satellite_id)
            .ok_or_else(|| anyhow!("Satellite not found: {}", cache_entry.satellite_id))?;

        let mut this = Self::new(handle);

        this.state_cache = cache_entry.cache;

        Ok(this)
    }

    fn latest_known_state(&self) -> Option<(StateCacheId, &StateCacheEntry)> {
        self.state_cache.get_latest()
    }
}

#[derive(Clone, Debug)]
struct PassData {
    first_state: StateCacheId,
    last_state: StateCacheId,
}

impl PassData {
    fn push_state(&mut self, state_id: StateCacheId) {
        assert!(state_id > self.last_state);

        self.last_state = state_id;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StateCache {
    /// Cached satellite states, sorted by time
    states: VecDeque<StateCacheEntry>,

    /// Any long-lived index into `states` will need to deal with the fact that
    /// the front of the deque will get items removed. So we track a
    /// monotonically increasing index and this defines the monotic index for
    /// the first element in the states deque.
    start_id: usize,
}

impl StateCache {
    pub fn get_earliest(&self) -> Option<(StateCacheId, &StateCacheEntry)> {
        Some((StateCacheId(self.start_id), self.states.front()?))
    }

    pub fn get_latest(&self) -> Option<(StateCacheId, &StateCacheEntry)> {
        Some((
            StateCacheId(self.start_id + self.states.len()),
            self.states.back()?,
        ))
    }

    pub fn get(&self, id: StateCacheId) -> Option<&StateCacheEntry> {
        let index = id.0.checked_sub(self.start_id)?;
        self.states.get(index)
    }

    pub fn remove_older_than(&mut self, before: DateTime<Utc>) {
        while let Some(entry) = self.states.front() {
            if entry.state.time() < before {
                self.states
                    .pop_front()
                    .expect("peek returned Some, but pop None");
                self.start_id += 1;
            }
            else {
                break;
            }
        }
    }

    pub fn push(&mut self, state: SatelliteState, relative: Option<RelativeState>) -> StateCacheId {
        // it would be nice if we could insert arbitrarly timed states, but that would
        // mess up the monotically increasing ids we had code here to do this
        // before. it just does a binary search and insert.

        if let Some((_, latest)) = self.get_latest() {
            assert!(state.time() > latest.state.time());
        }

        let index = self.states.len();
        self.states.push_back(StateCacheEntry { state, relative });

        StateCacheId(index + self.start_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StateCacheEntry {
    state: SatelliteState,
    #[serde(skip)]
    relative: Option<RelativeState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct StateCacheId(usize);

impl StateCacheId {
    pub fn is_successor_of(&self, other: &Self) -> bool {
        self.0 + 1 == other.0
    }

    pub fn is_predecessor_of(&self, other: &Self) -> bool {
        other.is_successor_of(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FileCacheEntry {
    satellite_id: satnogs::SatelliteId,
    #[serde(flatten)]
    cache: StateCache,
}
