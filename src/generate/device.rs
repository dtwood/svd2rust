use quote::Tokens;
use svd::Device;

use errors::*;
use util;
use Target;

use generate::{interrupt, peripheral};

/// Whole device generation
pub fn render(
    d: &Device,
    target: &Target,
    nightly: bool,
    device_x: &mut String,
) -> Result<Vec<Tokens>> {
    let mut out = vec![];

    let doc = format!(
        "Peripheral access API for {0} microcontrollers \
         (generated using svd2rust v{1})\n\n\
         You can find an overview of the API [here].\n\n\
         [here]: https://docs.rs/svd2rust/{1}/svd2rust/#peripheral-api",
        d.name.to_uppercase(),
        env!("CARGO_PKG_VERSION")
    );

    if *target == Target::Msp430 {
        out.push(quote! {
            #![feature(abi_msp430_interrupt)]
        });
    }

    match *target {
        Target::Msp430 | Target::RISCV => {
            out.push(quote! {
                #![cfg_attr(feature = "rt", feature(global_asm))]
                #![cfg_attr(feature = "rt", feature(use_extern_macros))]
                #![cfg_attr(feature = "rt", feature(used))]
            });
        }
        _ => {}
    }

    out.push(quote! {
        #![doc = #doc]
        // #![deny(missing_docs)]
        #![allow(dead_code)]
        // #![deny(warnings)]
        #![allow(non_camel_case_types)]
        // #![no_std]
    });

    if nightly {
        out.push(quote! {
            #![feature(untagged_unions)]
        });
    }

    match *target {
        Target::CortexM => {
            out.push(quote! {
                extern crate cortex_m;
                #[cfg(feature = "rt")]
                extern crate cortex_m_rt;
            });
        }
        Target::Msp430 => {
            out.push(quote! {
                extern crate msp430;
                #[cfg(feature = "rt")]
                extern crate msp430_rt;
                #[cfg(feature = "rt")]
                pub use msp430_rt::default_handler;
            });
        }
        Target::RISCV => {
            out.push(quote! {
                extern crate riscv;
                #[cfg(feature = "rt")]
                extern crate riscv_rt;
            });
        }
        Target::Ephy => {}
        Target::Edes => {}
        Target::None => {}
    }

    out.push(quote! {
        extern crate bare_metal;
        extern crate vcell;
    });

    // Retaining the previous assumption
    let mut fpu_present = true;

    if let Some(cpu) = d.cpu.as_ref() {
        let bits = util::unsuffixed(cpu.nvic_priority_bits as u64);

        out.push(quote! {
            /// Number available in the NVIC for configuring priority
            pub const NVIC_PRIO_BITS: u8 = #bits;
        });

        fpu_present = cpu.fpu_present;
    }

    out.extend(interrupt::render(target, &d.peripherals, device_x)?);

    let core_peripherals: &[&str];

    if fpu_present {
        core_peripherals = &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "FPU", "ITM", "MPU", "NVIC", "SCB", "SYST",
            "TPIU",
        ];
    } else {
        core_peripherals = &[
            "CBP", "CPUID", "DCB", "DWT", "FPB", "ITM", "MPU", "NVIC", "SCB", "SYST", "TPIU",
        ];
    }

    if *target == Target::CortexM {
        out.push(quote! {
            pub use cortex_m::peripheral::Peripherals as CorePeripherals;
            #[cfg(feature = "rt")]
            pub use cortex_m_rt::interrupt;
            #[cfg(feature = "rt")]
            pub use self::Interrupt as interrupt;
        });

        if fpu_present {
            out.push(quote! {
                pub use cortex_m::peripheral::{
                    CBP, CPUID, DCB, DWT, FPB, FPU, ITM, MPU, NVIC, SCB, SYST, TPIU,
                };
            });
        } else {
            out.push(quote! {
                pub use cortex_m::peripheral::{
                    CBP, CPUID, DCB, DWT, FPB, ITM, MPU, NVIC, SCB, SYST, TPIU,
                };
            });
        }
    }

    for p in &d.peripherals {
        if *target == Target::CortexM && core_peripherals.contains(&&*p.name.to_uppercase()) {
            // Core peripherals are handled above
            continue;
        }

        out.extend(peripheral::render(p, &d.peripherals, &d.defaults, nightly)?);
    }

    Ok(out)
}
