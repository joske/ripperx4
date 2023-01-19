use gtk::gio::compile_resources;

fn main() {
    compile_resources(
        "content",
        "content/ripperx4.gresource.xml",
        "ripperx4.gresource",
    );
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-L/opt/homebrew/lib/");
}
