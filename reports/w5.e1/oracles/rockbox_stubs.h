#ifndef ORACLE_STUBS_H
#define ORACLE_STUBS_H
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#define LCD_HEIGHT 240
#define MEMORYSIZE 64
#define UNICODE32 1
#define INIT_ATTR
#define NO_INLINE __attribute__((noinline))
#define LIKELY(x) (x)
#define PTR_ADD(p,n) ((void*)((char*)(p)+(n)))
#define MAX(a,b) ((a)>(b)?(a):(b))
#define ALIGN_BUFFER(p,n,a) ((void)0)
#define LRU_SLOT_OVERHEAD 0
#define BUFLIB_CB_OK 0
#define IS_DIACRITIC(x) 0
#define ROCKBOX_DIR "/tmp/fontpixel-rockbox-oracle"
#define MAX_PATH 4096
#define logf(...) ((void)0)
#define DEBUGF(...) ((void)0)
typedef uint32_t ucschar_t;
struct font_cache_entry { ucschar_t _char_code; int width; unsigned char bitmap[65536]; };
struct lru { void *_base; };
struct font_cache { void *_index; struct lru _lru; int _capacity; };
struct buflib_callbacks { int (*move)(int,void*,void*); void *shrink; void *sync; };
static inline void core_pin(int n){(void)n;}
static inline void core_unpin(int n){(void)n;}
static inline void *core_get_data(int n){(void)n;return NULL;}
static inline void *core_get_data_pinned(int n){(void)n;return NULL;}
static inline void core_put_data_pinned(void *p){(void)p;}
static inline int core_alloc_ex(size_t n,struct buflib_callbacks*c){(void)n;(void)c;return -1;}
static inline void core_free(int n){(void)n;}
static inline size_t strlcpy(char*d,const char*s,size_t n){size_t z=strlen(s);if(n){snprintf(d,n,"%s",s);}return z;}
static inline int ffilesize(int fd){struct stat s;return fstat(fd,&s)?-1:(int)s.st_size;}
static inline struct font_cache_entry *font_cache_get(struct font_cache*c,ucschar_t n,bool b,void(*f)(struct font_cache_entry*,void*),void*p){(void)c;(void)n;(void)b;(void)f;(void)p;return NULL;}
static inline void font_cache_create(struct font_cache*c,void*p,size_t n,int b){(void)c;(void)p;(void)n;(void)b;}
static inline void lru_traverse(struct lru*l,void(*f)(void*)){(void)l;(void)f;}
static inline const unsigned char*utf8decode(const unsigned char*p,ucschar_t*c){*c=*p;return p+1;}
#define panicf(...) abort()
#endif
