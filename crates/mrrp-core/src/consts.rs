macro_rules! make_consts {
    {$($name:ident = $value:expr;)*} => {
        pub mod f32 {
            $(pub const $name: f32 = $value;)*
        }

        pub mod f64 {
            $(pub const $name: f64 = $value;)*
        }

    };
}

make_consts! {
    C = 299_792_458.0;
}
