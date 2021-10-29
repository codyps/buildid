fn main() {
    env_logger::init();
    println!("build-id: {:?}", buildid::build_id())
}
