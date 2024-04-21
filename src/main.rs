use vulkan_testing::base::App;
use log::*;
use std::process;

fn main() {
    pretty_env_logger::init();

    let app = match App::create() {
        Ok(a) => a,
        Err(e) => {
            error!("Error creating app: {:?}", e);
            process::exit(1)
        }
    };

    let _ = app.render_loop(|| {});
}
