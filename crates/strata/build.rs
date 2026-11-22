// `embed_migrations!` reads `migrations/` at compile time: a new file must rebuild the crate.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
