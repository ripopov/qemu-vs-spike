/* Count QEMU guest instructions between the two CoreMark port marker NOPs. */
#include <inttypes.h>
#include <stdio.h>
#include <string.h>
#include <qemu-plugin.h>
QEMU_PLUGIN_EXPORT int qemu_plugin_version = QEMU_PLUGIN_VERSION;
static qemu_plugin_u64 count;
static uint64_t begin, end;
static unsigned starts, ends;
static void marker(unsigned cpu, void *opaque)
{
    if ((uintptr_t)opaque == 1) {
        begin = qemu_plugin_u64_get(count, cpu);
        starts++;
    } else {
        end = qemu_plugin_u64_get(count, cpu);
        ends++;
    }
}
static void translate(struct qemu_plugin_tb *tb, void *opaque)
{
    for (size_t i = 0; i < qemu_plugin_tb_n_insns(tb); i++) {
        struct qemu_plugin_insn *insn = qemu_plugin_tb_get_insn(tb, i);
        uint32_t opcode = 0;
        if (qemu_plugin_insn_size(insn) == 4) {
            qemu_plugin_insn_data(insn, &opcode, sizeof(opcode));
        }
        qemu_plugin_register_vcpu_insn_exec_inline_per_vcpu(
            insn, QEMU_PLUGIN_INLINE_ADD_U64, count, 1);
        if (opcode == 0x12300013 || opcode == 0x12400013) {
            qemu_plugin_register_vcpu_insn_exec_cb(insn, marker,
                QEMU_PLUGIN_CB_NO_REGS, (void *)(uintptr_t)(opcode == 0x12300013 ? 1 : 2));
        }
    }
}
static void finish(void *opaque)
{
    char buf[160];
    snprintf(buf, sizeof(buf), "BENCH_QEMU_ROI_INSNS=%" PRIu64 " starts=%u ends=%u\n",
             end - begin, starts, ends);
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
