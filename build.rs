extern crate prost_build;

fn main() {
    prost_build::compile_protos(&["proto/reflection.proto"], &["proto/"]).unwrap();
}
