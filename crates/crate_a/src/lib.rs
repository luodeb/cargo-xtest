pub fn crate_a_main() {
    #[cfg(SMP)]
    crate_b::crate_b_main();

    #[cfg(SMP)]
    {
        bitflags::bitflags! {
            #[derive(Debug)]
            struct MyFlags: u32 {
                const A = 0b0001;
                const B = 0b0010;
            }
        }
        let flags = MyFlags::A | MyFlags::B;
        println!("Crate A (smp, bitflags={:?})", flags);
    }
    #[cfg(not(SMP))]
    println!("Crate A (unsmp)");

    #[cfg(NET)]
    crate_net::crate_net_main();
    #[cfg(not(NET))]
    println!("Crate A (no net)");
}
