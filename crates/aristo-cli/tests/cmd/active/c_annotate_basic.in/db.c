// @aristo intent("open recovers the durable prefix, replaying intact records newest-write-wins", verify = "test", id = "db_open_recovers_durable_prefix")
int db_open(const char *dir) {
    return 0;
}

// @aristo intent("flush is the durability boundary the fault injector targets", verify = "neural", id = "db_flush_is_durability_boundary")
int db_flush(int fd) {
    return 0;
}
