pub fn crate_a_main() {
    #[cfg(xconfig = "smp")]
    crate_b::crate_b_main();

    #[cfg(xconfig = "smp")]
    println!("Crate A (smp)");
    #[cfg(not(xconfig = "smp"))]
    println!("Crate A (unsmp)");
}
