use std::fs::File;

use cairo_lang_sierra::program::Program;

fn main() {
    let mut args = std::env::args();
    args.next();

    let path = args.next().expect("expected cairo path as first argument");
    let file = File::open(path).expect("failed to create file");

    let program: Program = serde_json::from_reader(file).unwrap();

    println!("{}", program);
}
