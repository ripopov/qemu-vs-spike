#include "softfloat.h"
#include <stdio.h>
#include <inttypes.h>
int main(void) {
 unsigned fmt,op,rm; uint64_t a,b,c;
 while(scanf("%u %u %u %" SCNx64 " %" SCNx64 " %" SCNx64,&fmt,&op,&rm,&a,&b,&c)==6) {
  softfloat_roundingMode=rm; softfloat_exceptionFlags=0; softfloat_detectTininess=softfloat_tininess_afterRounding;
  uint64_t out=0;
  if(fmt==16) {
   float16_t x={a};
   out=op==5 ? f16_to_f32(x).v : f16_to_f64(x).v;
  } else if(fmt==32) {
   float32_t x={a},y={b},z={c};
   switch(op) {
    case 0:out=f32_add(x,y).v;break;case 1:out=f32_mul(x,y).v;break;case 2:out=f32_div(x,y).v;break;
    case 3:out=f32_sqrt(x).v;break;case 4:out=f32_mulAdd(x,y,z).v;break;
    case 5:out=f32_to_f64(x).v;break;
    case 14:out=f32_to_f16(x).v;break;
    case 6:out=(int64_t)(int32_t)f32_to_i32(x,rm,true);break;
    case 7:out=(int64_t)(int32_t)f32_to_ui32(x,rm,true);break;
    case 8:out=f32_to_i64(x,rm,true);break;case 9:out=f32_to_ui64(x,rm,true);break;
    case 10:out=i32_to_f32(a).v;break;case 11:out=ui32_to_f32(a).v;break;
    case 12:out=i64_to_f32(a).v;break;case 13:out=ui64_to_f32(a).v;break;
   }
  } else {
   float64_t x={a},y={b},z={c};
   switch(op) {
    case 0:out=f64_add(x,y).v;break;case 1:out=f64_mul(x,y).v;break;case 2:out=f64_div(x,y).v;break;
    case 3:out=f64_sqrt(x).v;break;case 4:out=f64_mulAdd(x,y,z).v;break;
    case 5:out=f64_to_f32(x).v;break;
    case 14:out=f64_to_f16(x).v;break;
    case 6:out=(int64_t)(int32_t)f64_to_i32(x,rm,true);break;
    case 7:out=(int64_t)(int32_t)f64_to_ui32(x,rm,true);break;
    case 8:out=f64_to_i64(x,rm,true);break;case 9:out=f64_to_ui64(x,rm,true);break;
    case 10:out=i32_to_f64(a).v;break;case 11:out=ui32_to_f64(a).v;break;
    case 12:out=i64_to_f64(a).v;break;case 13:out=ui64_to_f64(a).v;break;
   }
  }
  printf("%016" PRIx64 " %u\n",out,(unsigned)softfloat_exceptionFlags);
 }
 return ferror(stdin)?1:0;
}
