/*
 * malloc.c — First-fit free-list allocator for MYNEWOS user-space
 *
 * Uses brk() to grow the heap in 64 KiB chunks. Maintains a doubly-linked
 * free list so free() and realloc() actually work (important for DOOM).
 */

#include <stddef.h>

/* -------------------------------------------------------------------------- */
/* Raw kernel brk wrapper from syscall.S                                       */
/* -------------------------------------------------------------------------- */
extern long brk(long addr);

/* -------------------------------------------------------------------------- */
/* Internal heap state                                                          */
/* -------------------------------------------------------------------------- */
#define ALIGN       16                /* All allocations are 16-byte aligned  */
#define CHUNK_SIZE  (64 * 1024)       /* Grow heap by 64 KiB at a time        */
#define MAGIC_FREE  0xDEADBEEFUL
#define MAGIC_ALLOC 0xCAFEBABEUL

typedef struct Block {
    unsigned long   magic;  /* MAGIC_FREE or MAGIC_ALLOC                    */
    size_t          size;   /* payload size in bytes (not including header)  */
    struct Block   *prev;   /* previous block in free list                  */
    struct Block   *next;   /* next block in free list                      */
} Block;

#define HEADER_SIZE  (sizeof(Block))

static Block *free_list    = (Block *)0; /* head of free list               */
static char  *heap_start   = (char  *)0; /* first byte of managed heap      */
static char  *heap_end     = (char  *)0; /* current top of heap             */

/* Align a pointer/size upward to ALIGN bytes */
static inline size_t align_up(size_t n) {
    return (n + ALIGN - 1) & ~(size_t)(ALIGN - 1);
}

/* -------------------------------------------------------------------------- */
/* Grow the heap by at least `need` bytes. Returns 0 on failure.               */
/* -------------------------------------------------------------------------- */
static int grow_heap(size_t need) {
    size_t grow = need < CHUNK_SIZE ? CHUNK_SIZE : need;
    grow = align_up(grow);

    if (heap_start == (char *)0) {
        /* First allocation — query current break */
        long cur = brk(0);
        if (cur == -1) return 0;
        heap_start = (char *)cur;
        heap_end   = heap_start;
    }

    char *new_end = heap_end + grow;
    long result   = brk((long)new_end);

    /* Linux brk returns the new break on success. If it returned less than 
       requested, it means the allocation failed. */
    if (result < (long)new_end) return 0;

    /* Create one big free block covering the new region */
    Block *blk = (Block *)heap_end;
    blk->magic = MAGIC_FREE;
    blk->size  = grow - HEADER_SIZE;
    blk->prev  = (Block *)0;
    blk->next  = free_list;
    if (free_list) free_list->prev = blk;
    free_list = blk;

    heap_end = new_end;
    return 1;
}

/* -------------------------------------------------------------------------- */
/* malloc                                                                       */
/* -------------------------------------------------------------------------- */
void *malloc(size_t size) {
    if (size == 0) return (void *)0;
    size = align_up(size);

    /* Search free list for a suitable block */
    Block *b = free_list;
    while (b) {
        if (b->magic != MAGIC_FREE) {
            /* Heap corruption — just bail */
            return (void *)0;
        }
        if (b->size >= size) {
            /* Found a fit — split if there is enough space for a new block */
            if (b->size >= size + HEADER_SIZE + ALIGN) {
                Block *rest = (Block *)((char *)(b + 1) + size);
                rest->magic = MAGIC_FREE;
                rest->size  = b->size - size - HEADER_SIZE;
                rest->prev  = b->prev;
                rest->next  = b->next;
                if (rest->prev) rest->prev->next = rest;
                else            free_list        = rest;
                if (rest->next) rest->next->prev = rest;
                b->size = size;
            } else {
                /* Use the whole block */
                if (b->prev) b->prev->next = b->next;
                else         free_list     = b->next;
                if (b->next) b->next->prev = b->prev;
            }
            b->magic = MAGIC_ALLOC;
            b->prev  = (Block *)0;
            b->next  = (Block *)0;
            return (void *)(b + 1);
        }
        b = b->next;
    }

    /* No free block large enough — grow the heap */
    if (!grow_heap(size + HEADER_SIZE)) return (void *)0;
    return malloc(size); /* retry */
}

/* -------------------------------------------------------------------------- */
/* calloc                                                                       */
/* -------------------------------------------------------------------------- */
void *calloc(size_t nmemb, size_t size) {
    size_t total = nmemb * size;
    void *p = malloc(total);
    if (p) {
        /* Zero the memory */
        char *c = (char *)p;
        for (size_t i = 0; i < total; i++) c[i] = 0;
    }
    return p;
}

/* -------------------------------------------------------------------------- */
/* free                                                                         */
/* -------------------------------------------------------------------------- */
void free(void *ptr) {
    if (!ptr) return;
    Block *b = (Block *)ptr - 1;
    if (b->magic != MAGIC_ALLOC) return; /* double-free / bad pointer */
    b->magic = MAGIC_FREE;

    /* Prepend to free list */
    b->prev = (Block *)0;
    b->next = free_list;
    if (free_list) free_list->prev = b;
    free_list = b;
}

/* -------------------------------------------------------------------------- */
/* realloc                                                                      */
/* -------------------------------------------------------------------------- */
void *realloc(void *ptr, size_t size) {
    if (!ptr)  return malloc(size);
    if (!size) { free(ptr); return (void *)0; }

    Block *b = (Block *)ptr - 1;
    if (b->magic != MAGIC_ALLOC) return (void *)0;

    if (b->size >= size) return ptr; /* already large enough */

    /* Allocate new, copy, free old */
    void *new = malloc(size);
    if (!new) return (void *)0;
    char *src = (char *)ptr;
    char *dst = (char *)new;
    for (size_t i = 0; i < b->size; i++) dst[i] = src[i];
    free(ptr);
    return new;
}
