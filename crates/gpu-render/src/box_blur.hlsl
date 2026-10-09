// Existing premultiplied RGBA8 box blur: transparent borders, nearest-even
// byte quantization after each axis, no filtered texture sampling or FMA.
StructuredBuffer<uint> source : register(t0);
RWStructuredBuffer<uint> destination : register(u0);
cbuffer Params : register(b0) {
    uint width; uint height; uint axis; uint radius;
    float inverse; uint3 padding;
};
uint4 unpack(uint v) { return uint4(v & 255, (v >> 8) & 255, (v >> 16) & 255, v >> 24); }
uint pack(uint4 v) { return v.x | (v.y << 8) | (v.z << 16) | (v.w << 24); }
uint4 sample(int i, uint length, uint base, uint stride) {
    return i < 0 || i >= int(length) ? uint4(0, 0, 0, 0) : unpack(source[base + uint(i) * stride]);
}
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    uint lines = axis == 0 ? height : width;
    if (id.x >= lines) return;
    uint length = axis == 0 ? width : height;
    uint stride = axis == 0 ? 1 : width;
    uint base = axis == 0 ? id.x * width : id.x;
    uint4 sum = 0;
    for (uint j = 0; j < min(radius, length); ++j) sum += sample(int(j), length, base, stride);
    for (uint i = 0; i < length; ++i) {
        sum += sample(int(i + radius), length, base, stride);
        precise float4 scaled = float4(sum) * inverse;
        destination[base + i * stride] = pack(uint4(round(scaled)));
        sum -= sample(int(i) - int(radius), length, base, stride);
    }
}

// Adjacent lanes read adjacent pixels for both axes. Horizontal passes use
// a tiled transpose before and after the vertical sliding-window kernel.
groupshared uint tile[16][17];
[numthreads(16, 16, 1)]
void transpose(uint3 group : SV_GroupID, uint3 local : SV_GroupThreadID) {
    uint sx = group.x * 16 + local.x;
    uint sy = group.y * 16 + local.y;
    tile[local.y][local.x] = sx < width && sy < height ? source[sy * width + sx] : 0;
    GroupMemoryBarrierWithGroupSync();
    uint dx = group.y * 16 + local.x;
    uint dy = group.x * 16 + local.y;
    if (dx < height && dy < width)
        destination[dy * height + dx] = tile[local.x][local.y];
}
