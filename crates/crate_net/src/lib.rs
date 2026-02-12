pub fn crate_net_main() {
    println!("Hello, NET!");

    #[cfg(SMP)]
    println!("Crate NET (smp)");
    #[cfg(not(SMP))]
    println!("Crate NET (unsmp)");
}
