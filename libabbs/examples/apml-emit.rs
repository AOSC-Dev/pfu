use std::{
    env::{self, args},
    fs,
};

use libabbs::apml::{ast::{ApmlAst, AstNode}, lst::ApmlLst};

fn main() {
    let file = args().nth(1).expect("Usage: apml-emit <PATH>");
    let src = fs::read_to_string(&file).unwrap();
    let lst = ApmlLst::parse(&src).unwrap();
    let ast = ApmlAst::emit_from(&lst).unwrap();

    if env::var("QUIET").is_err() {
        println!("{:#?}", ast);
    }
}
