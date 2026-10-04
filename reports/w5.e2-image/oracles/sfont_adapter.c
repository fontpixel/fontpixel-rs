/* Original MIT adapter. SFont.c is compiled unchanged from the official project. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include "SFont.h"
int main(int argc,char **argv) {
    if(argc!=2)return 2;FILE *fp=fopen(argv[1],"rb");if(!fp)return 3;
    uint32_t wh[2];if(fread(wh,4,2,fp)!=2||!wh[0]||wh[0]>65536||wh[1]<2||wh[1]>4096)return 4;
    size_t n=(size_t)wh[0]*wh[1]*4;unsigned char *pixels=malloc(n);if(!pixels||fread(pixels,1,n,fp)!=n)return 5;fclose(fp);
    SDL_Surface *src=SDL_CreateRGBSurfaceWithFormatFrom(pixels,wh[0],wh[1],32,wh[0]*4,SDL_PIXELFORMAT_RGBA32);if(!src)return 6;
    SFont_Font *font=SFont_InitFont(src);if(!font)return 7;
    /* Copy coverage unchanged; blend mode is a documented caller-owned SDL setting. */
    SDL_SetSurfaceBlendMode(src,SDL_BLENDMODE_NONE);
    SDL_Surface *dst=SDL_CreateRGBSurfaceWithFormat(0,wh[0]+64,wh[1],32,SDL_PIXELFORMAT_RGBA32);if(!dst)return 8;
    for(int cp=33;cp<=126;cp++) {
        SDL_FillRect(dst,NULL,0);char text[2]={(char)cp,0};SFont_Write(dst,font,32,0,text);
        printf("GLYPH\t%d\t%d\t%d\t%d\t",cp,dst->w,dst->h-1,SFont_TextWidth(font,text));
        for(int y=0;y<dst->h-1;y++)for(int x=0;x<dst->w;x++)printf("%02x",((unsigned char*)dst->pixels)[y*dst->pitch+x*4+3]);puts("");
    }
    SDL_FreeSurface(dst);SFont_FreeFont(font);free(pixels);return 0;
}
