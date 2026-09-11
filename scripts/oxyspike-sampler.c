/* Linux/glibc process-local sampling: no perf_event access or target changes.
 * Load only into a diagnostic run, never a timing run.
 */
#define _GNU_SOURCE
#include <link.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <limits.h>
static unsigned short *samples;
static uintptr_t start_address, relative_address;
static size_t buckets;
static const char *output;
static int executable_segment(struct dl_phdr_info *info, size_t size, void *arg) {
    (void)size; (void)arg;
    if (info->dlpi_name && info->dlpi_name[0]) return 0;
    for (int n=0;n<info->dlpi_phnum;n++) {
        const ElfW(Phdr) *p=&info->dlpi_phdr[n];
        if (p->p_type==PT_LOAD && (p->p_flags&PF_X)) {
            start_address=info->dlpi_addr+p->p_vaddr;
            relative_address=p->p_vaddr;
            buckets=(p->p_memsz+63)/64;
            return 1;
        }
    }
    return 1;
}
__attribute__((constructor)) static void begin_sampling(void) {
    output=getenv("OXY_PROFILE_OUTPUT");
    if (!output || !*output) return;
    dl_iterate_phdr(executable_segment,0);
    if (!buckets) {fputs("sampler: no executable segment\n",stderr);return;}
    samples=calloc(buckets,sizeof(*samples));
    if (!samples) return;
    /* profil uses byte indices: scale=2048 maps each 64-byte PC span to
     * one two-byte counter (65536*2/64). */
    if (profil(samples,buckets*sizeof(*samples),start_address,2048)) {
        perror("sampler: profil");free(samples);samples=0;
    }
}
__attribute__((destructor)) static void finish_sampling(void) {
    if (!samples) return;
    profil(samples,0,0,0);
    FILE *f=fopen(output,"wx");
    if (!f) {perror("sampler: output");return;}
    fprintf(f,"# ELF-relative address, samples (64-byte buckets)\n");
    for (size_t n=0;n<buckets;n++) if (samples[n])
        fprintf(f,"0x%lx %u\n",(unsigned long)(relative_address+64*n),samples[n]);
    fclose(f);free(samples);
}
