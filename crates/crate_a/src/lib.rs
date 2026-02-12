pub fn crate_a_main() {
    #[cfg(SMP)]
    crate_b::crate_b_main();

    #[cfg(SMP)]
    println!("Crate A (smp)");
    #[cfg(not(SMP))]
    println!("Crate A (unsmp)");
}
