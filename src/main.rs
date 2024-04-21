use vulkan_testing::base::App;

fn main() {
    pretty_env_logger::init();

    let app = App::create().expect("Error creating app.");

    let _ = app.render_loop(|| {});
}
