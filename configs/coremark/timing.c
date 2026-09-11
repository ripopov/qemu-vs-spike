/* Local CoreMark port hooks. Algorithm sources remain unchanged. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
void __real_start_time(void);
void __real_stop_time(void);
static uint64_t begin_instret;
void __wrap_start_time(void)
{
    __real_start_time();
    puts("BENCH_COREMARK_START");
    fflush(stdout);
    __asm__ volatile(".4byte 0x12300013\n\trdinstret %0" : "=r"(begin_instret) :: "memory");
}
void __wrap_stop_time(void)
{
    uint64_t end_instret;
    __asm__ volatile("rdinstret %0\n\t.4byte 0x12400013" : "=r"(end_instret) :: "memory");
    puts("BENCH_COREMARK_END");
    fflush(stdout);
    __real_stop_time();
    /* This CSR is a retired-instruction counter on Spike, not normal QEMU TCG. */
    printf("BENCH_INSTRET_DELTA=%" PRIu64 "\n", end_instret - begin_instret);
}
