/* Known symbol used to check that the process-local sampler captures PCs. */
#include <stdint.h>
__attribute__((noinline,aligned(64))) uint64_t sampler_burn(uint64_t n) {
    volatile uint64_t x=1;
    for (uint64_t i=0;i<n;i++) x=x*6364136223846793005ULL+1;
    return x;
}
int main(void) {return sampler_burn(1000000000ULL)==0;}
