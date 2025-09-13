// Test CUDA kernel for verifying GPU framework
extern "C" {

__global__ void vector_add(const float* a, const float* b, float* c, int n) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        c[idx] = a[idx] + b[idx];
    }
}

__global__ void fill_buffer(float* buffer, float value, int n) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        buffer[idx] = value;
    }
}

__global__ void multiply_add(const float* a, const float* b, float scalar, float* c, int n) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        c[idx] = a[idx] * scalar + b[idx];
    }
}

// Simple hash function for testing (not cryptographic!)
__global__ void simple_hash(const uint32_t* input, uint32_t* output, uint32_t nonce_start, int n) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < n) {
        uint32_t nonce = nonce_start + idx;
        uint32_t hash = input[0] ^ nonce;
        
        // Simple mixing
        hash = hash * 0x85ebca6b;
        hash = hash ^ (hash >> 13);
        hash = hash * 0xc2b2ae35;
        hash = hash ^ (hash >> 16);
        
        output[idx] = hash;
    }
}

}