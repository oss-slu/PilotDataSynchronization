use itertools::Itertools;

#[no_mangle]
pub extern "C" fn meet() {
    println!("Hello, from Rust!");
    (0..2)
        .cartesian_product(0..2)
        .for_each(|x| println!("{x:?}"))
}
