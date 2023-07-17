pub mod server {
    include!(concat!(env!("OUT_DIR"), "/server.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut server = server::Person::default();
        server.name = "test".to_string();
        assert_eq!(server.name, "test");
    }
}
