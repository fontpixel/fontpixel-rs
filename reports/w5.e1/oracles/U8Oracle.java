/* Original public-API adapter, MIT. The external JAR supplies all font parsing. */
import java.io.RandomAccessFile;
import com.kreative.bitsnpicas.u8m.U8MFile;
import com.kreative.bitsnpicas.u8m.U8MGlyph;
public final class U8Oracle {
  public static void main(String[] args) throws Exception {
    try (RandomAccessFile input = new RandomAccessFile(args[0],"r")) {
      int base=0;if(input.readInt()!=U8MFile.MAGIC_NUMBER)base=2;
      input.seek(base);U8MFile f=new U8MFile();f.read(input,base);
      System.out.printf("FONT\t%d\t%d\t%d\t%d\t%d\t%d\t%d%n",f.familyID,f.style,f.pointSize,f.lineAscent,f.lineDescent,f.lineGap,f.lineHeight);
      for(int cp=0;cp<256;cp++){int i=f.getNativeGlyphIndex(cp);if(i!=0)System.out.printf("MAP\tlegacy\t%d\t%d%n",cp,i);}
      for(int cp=0;cp<=0x10ffff;cp++){int i=f.getUnicodeGlyphIndex(cp);if(i!=0)System.out.printf("MAP\tunicode\t%d\t%d%n",cp,i);}
      for(int i=0;i<f.glyphs.size();i++){
        U8MGlyph g=f.glyphs.get(i);System.out.printf("GLYPH\t%d\t%d\t%d\t%d\t%d\t%d\t",i,g.width,g.height,g.xOffset,-g.yOffset-g.height,g.advanceWidth);
        for(byte[] row:g.to2DArray())for(byte b:row)System.out.printf("%02x",b&255);
        System.out.println();
      }
    }
  }
}
