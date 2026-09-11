#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
// Each phase is chosen so that the counts of instructions, branches,
// mispredicts and L1D misses are known analytically.
static void phase_loop(uint64_t n) {            // 2 instructions/iter, 1 predictable taken branch
  __asm__ volatile("1: subs %0, %0, #1\n\tb.ne 1b" : "+r"(n) :: "cc");
}
static void phase_alu(uint64_t n) {             // 10 instructions/iter
  uint64_t a=0,b=0,c=0,d=0;
  __asm__ volatile("1: add %1,%1,#1\n\tadd %2,%2,#2\n\tadd %3,%3,#3\n\tadd %4,%4,#4\n\t"
                   "add %1,%1,#1\n\tadd %2,%2,#2\n\tadd %3,%3,#3\n\tadd %4,%4,#4\n\t"
                   "subs %0,%0,#1\n\tb.ne 1b" : "+r"(n),"+r"(a),"+r"(b),"+r"(c),"+r"(d) :: "cc");
}
static void phase_mispredict(uint64_t n, uint8_t* bits) { // ~n/2 mispredicts, 5 insns/iter + taken
  uint64_t acc=0;
  for (uint64_t i=0;i<n;i++) { if (bits[i&((1u<<26)-1)]) acc+=i; }
  if (acc==1) puts("x");
}
static void phase_miss(uint64_t n, uint64_t* chase) {    // n dependent loads, each a cache miss
  uint64_t p=0;
  for (uint64_t i=0;i<n;i++) p=chase[p];
  if (p==1) puts("x");
}
int main(int argc,char**argv){
  const char* mode=argv[1]; uint64_t n=strtoull(argv[2],0,0);
  if(!strcmp(mode,"loop")) phase_loop(n);
  else if(!strcmp(mode,"alu")) phase_alu(n);
  else if(!strcmp(mode,"mispredict")) {
    size_t m=1u<<26; uint8_t* bits=malloc(m); uint64_t s=88172645463325252ull;
    for(size_t i=0;i<m;i++){ s^=s<<13; s^=s>>7; s^=s<<17; bits[i]=s&1; }
    phase_mispredict(n,bits);
  } else if(!strcmp(mode,"miss")) {
    size_t m=(1u<<24); uint64_t* chase=malloc(m*8); // 128 MiB, random cycle
    for(size_t i=0;i<m;i++) chase[i]=i;
    uint64_t s=1234567; for(size_t i=m-1;i>0;i--){ s^=s<<13; s^=s>>7; s^=s<<17; size_t j=s%(i+1); uint64_t t=chase[i]; chase[i]=chase[j]; chase[j]=t; }
    // make a single cycle (Sattolo-like): rotate so chase[i] points to next in permutation order
    uint64_t* nxt=malloc(m*8); for(size_t i=0;i<m;i++) nxt[chase[i]]=chase[(i+1)%m];
    phase_miss(n,nxt);
  }
  return 0;
}
