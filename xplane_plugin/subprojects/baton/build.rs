fn main() {
    let mut bridge = cxx_build::bridge("src/lib.rs");
    bridge.std("c++20");

    if let Ok(target) = std::env::var("BATON_TARGET") {
        if !target.is_empty() {
            bridge.target(&target);
        }
    }

    bridge.compile("baton");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-env-changed=BATON_TARGET");
}
