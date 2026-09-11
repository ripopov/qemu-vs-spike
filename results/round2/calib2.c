#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
int main(int argc,char**argv){
  uint64_t n=strtoull(argv[1],0,0); size_t m=1u<<26; uint8_t* bits=malloc(m); uint64_t s=88172645463325252ull;
  for(size_t i=0;i<m;i++){ s^=s<<13; s^=s>>7; s^=s<<17; bits[i]=s&1; }
  uint64_t acc=0;
  for (uint64_t i=0;i<n;i++) {
    uint8_t b=bits[i&(m-1)];
    __asm__ volatile("cbz %w1, 1f\n\tadd %0,%0,%2\n1:" : "+r"(acc) : "r"(b), "r"(i) : "cc"); // real, unpredictable branch
  }
  if(acc==1) puts("x"); return 0;
}
