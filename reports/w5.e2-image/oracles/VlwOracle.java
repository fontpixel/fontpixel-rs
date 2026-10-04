/* Original MIT adapter using Processing's public/protected PFont APIs. */
import processing.core.PFont;
import processing.core.PImage;
import processing.core.PConstants;
import java.io.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
public final class VlwOracle extends PFont {
    VlwOracle(InputStream input) throws IOException { super(input); }
    VlwOracle() {
        super(); size=9; ascent=7; descent=2; smooth=true;
        name="Original\0🙂"; psname="OriginalPS";
        glyphCount=3; glyphs=new Glyph[3];
        int[][] fields={{65,3,2,5,4,-2},{256,2,3,4,6,1},{0x20001,0,0,7,0,0}};
        int[][] samples={{0,17,255,64,128,240},{255,0,1,254,128,0},{}};
        for(int i=0;i<3;i++) {
            Glyph g=new Glyph();int[] f=fields[i];g.value=f[0];g.width=f[1];g.height=f[2];
            g.setWidth=f[3];g.topExtent=f[4];g.leftExtent=f[5];g.index=i;
            g.image=new PImage(g.width,g.height,PConstants.ALPHA);
            System.arraycopy(samples[i],0,g.image.pixels,0,samples[i].length);glyphs[i]=g;
        }
    }
    static String hex(byte[] values) {
        StringBuilder out=new StringBuilder();for(byte value:values)out.append(String.format("%02x",value&255));return out.toString();
    }
    void dump() {
        System.out.printf("FONT\t%d\t%d\t%d\t%b\t%s\t%s%n",size,ascent,descent,smooth,hex((name==null?"":name).getBytes(StandardCharsets.UTF_8)),hex((psname==null?"":psname).getBytes(StandardCharsets.UTF_8)));
        for(int i=0;i<getGlyphCount();i++) {
            Glyph g=getGlyph(i);byte[] pixels=new byte[g.image.pixels.length];for(int j=0;j<pixels.length;j++)pixels[j]=(byte)g.image.pixels[j];
            System.out.printf("GLYPH\t%d\t%d\t%d\t%d\t%d\t%d\t%s%n",g.value,g.width,g.height,g.leftExtent,g.topExtent-g.height,g.setWidth,hex(pixels));
        }
    }
    public static void main(String[] args) throws Exception {
        if(args[0].equals("emit")){try(OutputStream out=Files.newOutputStream(Path.of(args[1]))){new VlwOracle().save(out);}return;}
        VlwOracle font;try(InputStream in=Files.newInputStream(Path.of(args[1]))){font=new VlwOracle(in);}
        if(args[0].equals("inspect"))font.dump();
        else if(args[0].equals("rewrite")){try(OutputStream out=Files.newOutputStream(Path.of(args[2]))){font.save(out);}}
        else throw new IllegalArgumentException("emit PATH | inspect PATH | rewrite IN OUT");
    }
}
