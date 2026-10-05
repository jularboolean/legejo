// Recompile when migrations change, so sqlx::migrate! embeds the latest files.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
