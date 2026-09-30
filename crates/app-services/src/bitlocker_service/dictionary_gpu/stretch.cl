// BitLocker SHA-256 stretch: exactly 0x100000 rounds, sliced by the host for
// Windows watchdog/cancellation. SHA words are numeric big-endian on both sides.
__constant uint K[64] = {
  0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
  0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
  0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
  0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
  0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
  0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
  0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
  0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};
#define RR(x,n) rotate((uint)(x),(uint)(32-(n)))
#define S0(x) (RR(x,7)^RR(x,18)^((x)>>3))
#define S1(x) (RR(x,17)^RR(x,19)^((x)>>10))
uint swap32(uint x) { return ((x&255)<<24)|((x&65280)<<8)|((x>>8)&65280)|(x>>24); }
void compress(uint *h, uint *w) {
  uint a=h[0],b=h[1],c=h[2],d=h[3],e=h[4],f=h[5],g=h[6],v=h[7];
  // Constant indices keep the rolling schedule in registers instead of scratch.
  #pragma unroll
  for (uint t=0;t<64;t++) {
    uint j=t&15;
    if(t>=16) w[j]+=S0(w[(t+1)&15])+w[(t+9)&15]+S1(w[(t+14)&15]);
    uint t1=v+(RR(e,6)^RR(e,11)^RR(e,25))+bitselect(g,f,e)+K[t]+w[j];
    uint t2=(RR(a,2)^RR(a,13)^RR(a,22))+((a&b)|(c&(a|b)));
    v=g;g=f;f=e;e=d+t1;d=c;c=b;b=a;a=t1+t2;
  }
  h[0]+=a;h[1]+=b;h[2]+=c;h[3]+=d;h[4]+=e;h[5]+=f;h[6]+=g;h[7]+=v;
}
__kernel void stretch(__global const uint *initial, __global uint *last,
                      __global const uint *salt, uint start, uint rounds) {
  size_t id=get_global_id(0)*8;
  uint previous[8], first[8];
  for(uint i=0;i<8;i++){previous[i]=last[id+i];first[i]=initial[id+i];}
  for(uint count=start;count<start+rounds;count++) {
    uint h[8]={0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
               0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    uint w[16];
    for(uint i=0;i<8;i++){w[i]=previous[i];w[i+8]=first[i];}
    compress(h,w);
    for(uint i=0;i<4;i++)w[i]=salt[i];
    w[4]=swap32(count);w[5]=0;w[6]=0x80000000;
    for(uint i=7;i<15;i++)w[i]=0;
    w[15]=704;
    compress(h,w);
    for(uint i=0;i<8;i++)previous[i]=h[i];
  }
  for(uint i=0;i<8;i++)last[id+i]=previous[i];
}
