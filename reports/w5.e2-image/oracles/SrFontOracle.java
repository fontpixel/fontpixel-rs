/* Original MIT public-API adapter. The external JAR owns all native coding. */
import com.kreative.bitsnpicas.*;
import com.kreative.bitsnpicas.importer.SRFontBitmapFontImporter;
import com.kreative.bitsnpicas.exporter.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
public final class SrFontOracle {
  static BitmapFont original(boolean rfont,boolean overhang) {
    BitmapFont f=new BitmapFont(5,2,7,3,4,5,1,7);
    f.setName(Font.NAME_FAMILY,"Original é");f.setName(Font.NAME_STYLE,"Regular");f.setName(Font.NAME_COPYRIGHT,"MIT original pixels");
    int[] cps=rfont?new int[]{65,256,0x20001}:new int[]{33,65,120,121,126};
    for(int i=0;i<cps.length;i++) {
      byte[][] p=new byte[4][3];for(int y=0;y<4;y++)for(int x=0;x<3;x++)p[y][x]=(byte)((x*17+y*31+i*11)%256);
      p[0][0]=(byte)255;p[3][2]=(byte)128;
      f.putCharacter(cps[i],new BitmapFontGlyph(p,!rfont&&!overhang&&i==0?1:i%2==0?-1:1,5,6));
    } return f;
  }
  static String hex(String s){if(s==null)return "-";StringBuilder b=new StringBuilder();for(byte x:s.getBytes(StandardCharsets.UTF_8))b.append(String.format("%02x",x&255));return b.toString();}
  static void dump(BitmapFont f){
    System.out.printf("FONT\t%d\t%d\t%d\t%d\t%d\t%d\t%s\t%s\t%s%n",f.getLineAscent(),f.getLineDescent(),f.getEmAscent(),f.getEmDescent(),f.getXHeight(),f.getLineGap(),hex(f.getName(Font.NAME_FAMILY)),hex(f.getName(Font.NAME_STYLE)),hex(f.getName(Font.NAME_COPYRIGHT)));
    for(int cp:f.characters(false).keySet()){BitmapFontGlyph g=f.getCharacter(cp);System.out.printf("GLYPH\t%d\t%d\t%d\t%d\t%d\t%d\t",cp,g.getGlyphWidth(),g.getGlyphHeight(),g.getGlyphOffset(),-g.getGlyphDescent(),g.getCharacterWidth());for(byte[] row:g.getGlyph())for(byte p:row)System.out.printf("%02x",p&255);System.out.println();}
  }
  public static void main(String[] args)throws Exception{
    if(args[0].equals("emit")){boolean r=args[1].equals("rfont");BitmapFontExporter e=r?new RFontBitmapFontExporter(0xffffff):new SFontBitmapFontExporter(0xffffff);e.exportFontToFile(original(r,args[1].equals("sfont-overhang")),new File(args[2]));return;}
    BitmapFont f=new SRFontBitmapFontImporter().importFont(new File(args[1]))[0];
    if(args[0].equals("inspect"))dump(f);else throw new IllegalArgumentException();
  }
}
