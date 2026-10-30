// @aristo assume("external café", id = "c_external")
int work(void) {
    if (1) {
        // @aristo intent("duplicate claim", parent = "c_external")
        int first = 1;
    }
    return 0;
}
