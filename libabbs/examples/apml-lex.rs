use std::{
    env::{self, args},
    fs,
};

fn main() {
    let file = args().nth(1).expect("Usage: apml-lex <PATH>");
    let src = fs::read_to_string(&file).unwrap();
    let tree = libabbs::apml::tree::ApmlParseTree::parse(&src).unwrap();

    // validation
    assert_eq!(tree.to_string(), src);

    if env::var("QUIET").is_err() {
        println!("{:#?}", tree);
    }
}
