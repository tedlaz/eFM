//! Embeds the icon into the .exe itself, so that Explorer and the taskbar show
//! it. The window icon is set separately in `main.rs`: winit declares the window
//! class with `hIcon: 0` and does not read the resource.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            // Without the Windows SDK there is no rc.exe; the build carries on
            // without an icon in the exe instead of failing.
            println!("cargo:warning=could not embed the icon into the exe: {e}");
        }
    }
}
