fn main() {
    cxx_build::bridge("src/lib.rs").compile("ipc-cpp-rs-demo");

    println!("cargo:rerun-if-changed=src/lib.rs");
}
