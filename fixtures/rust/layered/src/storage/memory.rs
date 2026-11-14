pub fn read() {}

#[cfg(test)]
mod tests {
    #[test]
    fn the_test_knows_the_route() {
        assert_eq!(crate::api::http::ROUTE, "/orders");
    }
}
