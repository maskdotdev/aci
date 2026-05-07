use std::fmt::Display;

pub struct Service;

impl Service {
    pub fn run(&self) {
        println!("run");
    }
}

pub enum Mode {
    Fast,
}

pub trait Runner {
    fn execute(&self);
}

pub fn main() {
    let service = Service;
    service.run();
}
