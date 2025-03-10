fn main() {
    cxx_build::bridge("src/lib.rs")
        .std("c++20")
        .compiler("x86_64-w64-mingw32-g++")
        .target("x86_64-pc-windows-gnu")
        .compile("baton");

    println!("cargo:rerun-if-changed=src/lib.rs");
}
