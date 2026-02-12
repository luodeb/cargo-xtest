pub fn crate_b_main() {
    #[cfg(feature = "smp")]
    println!("Crate B (smp)");
    #[cfg(not(feature = "smp"))]
    println!("Crate B (unsmp)");
    #[cfg(feature = "smp")]
    crate_smp::crate_smp_main();
}
