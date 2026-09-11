/* Local experiment: count dispatched guest instructions without register reads. */
#include <inttypes.h>
#include <stdio.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static qemu_plugin_u64 count;
static void translate(struct qemu_plugin_tb *tb, void *opaque)
{
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        qemu_plugin_register_vcpu_insn_exec_inline_per_vcpu(
            qemu_plugin_tb_get_insn(tb, i), QEMU_PLUGIN_INLINE_ADD_U64, count, 1);
    }
}
static void finish(void *opaque)
{
    char buf[128];
    snprintf(buf, sizeof(buf), "total insns: %" PRIu64 "\n", qemu_plugin_u64_sum(count));
    qemu_plugin_outs(buf);
    qemu_plugin_scoreboard_free(count.score);
}
QEMU_PLUGIN_EXPORT int qemu_plugin_install(qemu_plugin_id_t id,
                                          const qemu_info_t *info,
                                          int argc, char **argv)
{
    count.score = qemu_plugin_scoreboard_new(sizeof(uint64_t));
    count.offset = 0;
    qemu_plugin_register_vcpu_tb_trans_cb(id, translate, NULL);
    qemu_plugin_register_atexit_cb(id, finish, NULL);
    return 0;
}
