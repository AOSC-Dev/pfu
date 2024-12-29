use std::{
    env::{self, args},
    fs,
};

fn main() {
    let file = args().nth(1).expect("Usage: apml-eval <PATH>");
    let src = fs::read_to_string(&file).unwrap();
    let ctx = libabbs::apml::ApmlContext::parse(&src).unwrap();

    if env::var("QUIET").is_err() {
        println!("{:#?}", ctx);
    }
}
