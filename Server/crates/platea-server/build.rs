// Le migrazioni sono incluse nel binario: questo file le fa ricompilare quando cambiano.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
