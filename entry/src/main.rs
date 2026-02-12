fn main() {
    crate_a::crate_a_main();

    #[cfg(SMP)]
    println!("Entry (smp)");
    #[cfg(not(SMP))]
    println!("Entry (unsmp)");
}
