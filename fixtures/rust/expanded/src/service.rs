#[derives::logged]
pub fn bill(invoice: &crate::model::Invoice) -> u64 {
    let _started = crate::now!();
    invoice.total
}
