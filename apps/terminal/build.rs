// ABOUTME: Build script that compiles .slint UI files into Rust code.
// ABOUTME: Generates type-safe Rust bindings from the declarative UI definitions.

fn main() {
    slint_build::compile("ui/terminal.slint").unwrap();
}
