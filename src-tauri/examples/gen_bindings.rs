fn main() {
    lingq_upload_lib::export_bindings().expect("export_bindings");
    println!("wrote src/lib/ipc/bindings.ts");
}
