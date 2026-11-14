use facade::Motor;
use std::time::Duration;

pub fn run() -> (Motor, Duration) {
    let drive = facade::Drive::start();
    (drive, Duration::from_secs(1))
}
