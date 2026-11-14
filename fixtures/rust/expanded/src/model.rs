use derives::Stored;

#[derive(Stored, Debug)]
pub struct Invoice {
    pub total: u64,
}
