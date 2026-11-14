extern crate getrandom; //~ extern-crate getrandom
extern crate rand as dice;

pub fn roll() -> u32 {
    use dice::Rng;
    dice::thread_rng().next_u32() //~ expr rand::thread_rng
}
