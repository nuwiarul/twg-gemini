fn main() {
    // Recompile twg-server whenever SQLx migration files are added, removed, or modified.
    println!("cargo:rerun-if-changed=migrations");
}
