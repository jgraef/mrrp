use anyhow::Error;
use chrono::{
    DateTime,
    Utc,
};
use mrrp_geo::{
    Geodetic,
    Qth,
};
use mrrp_sat::satellite::{
    OrbitPropagationCache,
    ReferenceState,
    Satellite,
};

use crate::util::si;

pub fn show(
    satellite: &Satellite,
    time: DateTime<Utc>,
    location: Option<Geodetic>,
    frequencies: &[f64],
) -> Result<(), Error> {
    let state = {
        let mut output = vec![];
        match satellite.predict_state_into(
            &[time],
            &mut OrbitPropagationCache::default(),
            &mut output,
        ) {
            Ok(()) => {
                assert_eq!(output.len(), 1);
                output.pop()
            }
            Err(error) => {
                tracing::error!(%error);
                None
            }
        }
    };

    println!("Name:              {}", satellite.name());
    if let Some(norad_cat_id) = satellite.norad_cat_id() {
        println!("NORAD Category ID: {}", norad_cat_id);
    }
    else {
        println!("NORAD Category ID: Not assigned");
    }

    if let Some(state) = state {
        let (orbit_num, orbit_phase) = satellite
            .most_recent_tle()
            .unwrap()
            .orbit_number_and_phase(time);

        let geodetic = state.geodetic();
        println!("Latitude :         {:.4}°", geodetic.latitude);
        println!("Longitude:         {:.4}°", geodetic.longitude);
        println!(
            "Altitude:          {:.0}",
            si::format(geodetic.altitude, "m")
        );
        println!(
            "QTH locator:       {}",
            Qth::from_geodetic(geodetic.as_horizontal(), true)
        );

        println!("Orbit Number:      {orbit_num}");
        println!("Orbit Phase:       {:.0}°", orbit_phase.to_degrees());

        if let Some(location) = location {
            let relative = state.relative(&ReferenceState::from_geodetic(location));

            println!("Distance:          {:.0} km", relative.distance() / 1000.0);
            println!("Azimuth:           {:.2}°", relative.azimuth().to_degrees());
            println!(
                "Elevation:         {:.2}°",
                relative.elevation().to_degrees()
            );

            println!(
                "Radial speed:      {:.3} km/s",
                relative.radial_speed() / 1000.0
            );

            println!(
                "Signal delay:      {:.3} ms",
                relative.signal_delay().as_secs_f64() * 1000.0
            );

            for frequency in frequencies {
                println!("For {:.3}:", si::format(*frequency, "Hz"),);
                println!(
                    "  Free space path loss: {:.2} dB",
                    relative.free_space_path_loss_db(*frequency)
                );
                println!(
                    "  Doppler shift:        {:.3}",
                    si::format(relative.doppler_shift(*frequency), "Hz")
                );
            }
        }
    }

    Ok(())
}
