fn main() {
    crate_a::crate_a_main();

    #[cfg(xconfig = "smp")]
    println!("Entry (smp)");
    #[cfg(not(xconfig = "smp"))]
    println!("Entry (unsmp)");
}
