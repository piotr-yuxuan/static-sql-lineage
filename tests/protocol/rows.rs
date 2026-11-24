// SPDX-License-Identifier: EUPL-1.2
//! The macro of rows, which `tests/concepts.rs` loads too.

/// One test per row, `name: body;`, named after the case: a scenario's identifier, `.`
/// and `-` becoming `_`. The row's attributes, such as `ignore`, pass through.
macro_rules! cases {
    ($($(#[$attribute:meta])* $name:ident: $body:expr;)*) => {
        $(
            #[test]
            $(#[$attribute])*
            fn $name() {
                $body;
            }
        )*
    };
}
