fn main() {
    cxx_build::bridge("src/lib.rs")
        .std("c++20")
        .compile("ipc-cpp-rs-demo");

    println!("cargo:rerun-if-changed=src/lib.rs");
}
