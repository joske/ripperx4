use glib_build_tools::compile_resources;

fn main() {
    compile_resources(
        &["content"],
        "content/ripperx4.gresource.xml",
        "ripperx4.gresource",
    );
}
