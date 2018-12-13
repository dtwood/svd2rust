use quote::Tokens;
use svd::Device;
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedUpperCase};
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

    let mut fields = vec![];
    let mut exprs = vec![];
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

        if p.registers
            .as_ref()
            .map(|v| &v[..])
            .unwrap_or(&[])
            .is_empty()
            && p.derived_from.is_none()
        {
            // No register block will be generated so don't put this peripheral
            // in the `Peripherals` struct
            continue;
        }

        let p = p.name.to_sanitized_upper_case();
        let id = Ident::new(&*p);
        fields.push(quote! {
            #[doc = #p]
            pub #id: (#id, #id, #id, #id, #id, #id, #id, #id, #id, #id, #id, #id, #id, #id, #id, #id)
        });
        exprs.push(quote!(
            #id: (
                #id::new(0),
                #id::new(1),
                #id::new(2),
                #id::new(3),
                #id::new(4),
                #id::new(5),
                #id::new(6),
                #id::new(7),
                #id::new(8),
                #id::new(9),
                #id::new(10),
                #id::new(11),
                #id::new(12),
                #id::new(13),
                #id::new(14),
                #id::new(15),
            )
        ));
    }

    let take = match *target {
        Target::CortexM => Some(Ident::new("cortex_m")),
        Target::Msp430 => Some(Ident::new("msp430")),
        Target::RISCV => Some(Ident::new("riscv")),
        Target::Ephy => Some(Ident::new("cortex_m")),
        Target::None => None,
    }
    .map(|krate| {
        quote! {
            /// Returns all the peripherals *once*
            #[inline]
            pub fn take() -> Option<Self> {
                #krate::interrupt::free(|_| {
                    if unsafe { DEVICE_PERIPHERALS_EPHY } {
                        None
                    } else {
                        Some(unsafe { Peripherals::steal() })
                    }
                })
            }
        }
    });

    out.push(quote! {
        // NOTE `no_mangle` is used here to prevent linking different minor versions of the device
        // crate as that would let you `take` the device peripherals more than once (one per minor
        // version)
        #[allow(renamed_and_removed_lints)]
        // This currently breaks on nightly, to be removed with the line above once 1.31 is stable
        #[allow(private_no_mangle_statics)]
        #[no_mangle]
        static mut DEVICE_PERIPHERALS_EPHY: bool = false;

        /// All the peripherals
        #[allow(non_snake_case)]
        pub struct Peripherals {
            #(#fields,)*
        }

        impl Peripherals {
            #take

            /// Unchecked version of `Peripherals::take`
            pub unsafe fn steal() -> Self {
                debug_assert!(!DEVICE_PERIPHERALS_EPHY);

                DEVICE_PERIPHERALS_EPHY = true;

                Peripherals {
                    #(#exprs,)*
                }
            }
        }
    });

    Ok(out)
}
