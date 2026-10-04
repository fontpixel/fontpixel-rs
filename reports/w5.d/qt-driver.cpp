// Original test driver. Qt implementations are linked from the external build.
#include <QtCore/QCoreApplication>
#include <QtCore/QString>
#include <QtGui/QImage>
#include <private/qfont_p.h>
#include <private/qfontengine_p.h>
#include <private/qfontengine_qpf_p.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
int main(int argc, char **argv) {
    QCoreApplication app(argc,argv);
    if(argc<4) return 2;
    QFontDef def;
    QFontEngine *engine=0;
    if(!std::strcmp(argv[1],"qpf")) engine=new QFontEngineQPF1(def,QString::fromLocal8Bit(argv[2]));
    else {
        int fd=::open(argv[2],O_RDONLY);
        if(fd<0)return 3;
        QFontEngineQPF *font=new QFontEngineQPF(def,fd);
        if(!font->isValid())return 4;
        engine=font;
    }
    std::printf("{\"ascent\":%.8g,\"descent\":%.8g,\"leading\":%.8g,\"glyphs\":[",engine->ascent().toReal(),engine->descent().toReal(),engine->leading().toReal());
    bool mappings=false;bool comma=false;
    for(int i=3;i<argc;++i){
        if(!std::strcmp(argv[i],"--")){std::printf("],\"mapping\":[");mappings=true;comma=false;continue;}
        unsigned value=std::strtoul(argv[i],0,0);
        if(comma)std::printf(",");comma=true;
        if(mappings){QString str=QString::fromUcs4(&value,1);QGlyphLayoutArray<4> layout;int n=4;bool ok=engine->stringToCMap(str.constData(),str.size(),&layout,&n,QTextEngine::ShaperFlags());if(!ok||n!=1)return 5;std::printf("[%u,%u]",value,layout.glyphs[0]);continue;}
        glyph_metrics_t m=engine->boundingBox(value);
        int w=m.width.toInt(),h=m.height.toInt();
        QImage image=(w&&h)?engine->alphaMapForGlyph(value):QImage();
        std::printf("{\"native_id\":%u,\"width\":%d,\"height\":%d,\"x\":%.8g,\"y_down\":%.8g,\"advance\":%.8g,\"alpha\":\"",value,w,h,m.x.toReal(),m.y.toReal(),m.xoff.toReal());
        for(int y=0;y<h;++y)for(int x=0;x<w;++x){const uchar *row=image.constScanLine(y);unsigned sample=image.format()==QImage::Format_Mono?((row[x/8]>>(7-x%8))&1)*255:row[x];std::printf("%02x",sample);}
        std::printf("\"}");
    }
    std::printf("]}\n");delete engine;return 0;
}
