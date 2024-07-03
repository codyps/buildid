fn main() {
    env_logger::init();
    println!("build-id: {:X?}", buildid::build_id())
}
