use logkit::emit; //~ use logkit::emit

pub fn log() {
    logkit::emit!("started"); //~ macro logkit::emit!
    emit!("stopped"); //~ macro logkit::emit!
}
