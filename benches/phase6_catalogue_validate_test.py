#!/usr/bin/env python3
"""Small regressions for independently observable catalogue validation gaps."""
import copy
import struct
from types import SimpleNamespace
import unittest
from phase6_catalogue_bitmap import bdf_physical, pcf_physical, compare_profiles
from phase6_catalogue_metadata import bitmap_properties, name_records
from phase6_catalogue_validate import glyphs


class CompleteBitmapChecks(unittest.TestCase):
    def font(self):
        return SimpleNamespace(props={'FAMILY_NAME':'Family','WEIGHT_NAME':'Regular','SLANT':'R','COPYRIGHT':'Original notice'},
                               pixel_size=14,ascent=12,descent=2,bbox=(14,14,0,-2))

    def test_every_previously_missed_name_and_global_metric_is_rejected(self):
        original=self.font();task={'file':'fixture.bdf.gz'}
        for key,value in [('FAMILY_NAME','Wrong family'),('WEIGHT_NAME','Bold'),('SLANT','I')]:
            with self.subTest(key=key):
                changed=copy.deepcopy(original);changed.props[key]=value
                with self.assertRaises(AssertionError):bitmap_properties(task,changed,original)
        for key,value in [('pixel_size',99),('ascent',99),('descent',99),('bbox',(1,2,3,4))]:
            with self.subTest(key=key):
                changed=copy.deepcopy(original);setattr(changed,key,value)
                with self.assertRaises(AssertionError):bitmap_properties(task,changed,original)
        changed=copy.deepcopy(original);del changed.props['COPYRIGHT']
        with self.assertRaises(AssertionError):bitmap_properties(task,changed,original)
        self.assertEqual(bitmap_properties(task,original,original)['global_metrics']['ascent'],12)

    def test_encoded_name_is_part_of_application_glyph_comparison(self):
        a=SimpleNamespace(cp=65,name='A',bbw=1,bbh=1,bbx=0,bby=0,dwidth=1,rows=b'\x80')
        b=copy.copy(a);b.name='wrong'
        self.assertNotEqual(glyphs(SimpleNamespace(glyphs=[a])),glyphs(SimpleNamespace(glyphs=[b])))

    def profile(self,physical=None):
        return {'headers':{'fontname':'Original FONT','pointsize':14,'xres':75,'yres':75},
                'properties':{'copyright':'Original notice'},'property_spelling':{'copyright':'COPYRIGHT'},
                'encoded_records':{65:['A',65,1,1,0,0,1000,0,1,0,None,None,None,None,None,None,['80']]},
                'unencoded_records':[],
                'physical':physical or {'physical_glyphs':[]}}

    def test_font_header_and_swidth_are_not_projected_away(self):
        a=self.profile();b=copy.deepcopy(a);b['headers']['fontname']='Wrong FONT'
        b['encoded_records'][65][6]=999
        diff=compare_profiles(a,b)
        self.assertIn('fontname',diff['headers']);self.assertIn('65',diff['encoded_records'])

    def test_duplicate_bdf_records_and_unknown_glyph_properties_survive(self):
        glyph='STARTCHAR A\nENCODING 65\nBBX 1 1 0 0\nDWIDTH 1 0\nPRIVATE notice\nBITMAP\n80\nENDCHAR\n'
        raw=('STARTFONT 2.1\nCHARS 2\n'+glyph+glyph+'ENDFONT\n').encode()
        original=bdf_physical(raw)
        self.assertEqual(len(original['physical_glyphs']),2)
        self.assertEqual(original['physical_glyphs'][0]['rows'],['80'])
        changed_pixels=bdf_physical(raw.replace(b'\n80\n',b'\n00\n',1))
        self.assertIn('physical_glyph_records',compare_profiles(self.profile(original),self.profile(changed_pixels)))
        changed=copy.deepcopy(original);changed['physical_glyphs'].pop()
        diff=compare_profiles(self.profile(original),self.profile(changed))
        self.assertEqual(diff['physical_glyph_records']['native_only'][0]['count'],1)
        self.assertIn(['PRIVATE',['notice']],original['physical_glyphs'][0]['fields'])
        with self.assertRaises(ValueError):bdf_physical(raw.replace(b'CHARS 2',b'CHARS 1'))

    def test_pcf_physical_unencoded_slot_is_not_an_empty_projection(self):
        metrics=struct.pack('<IH',0x100,1)+b'\x80'*5
        encoding=struct.pack('<I6H',0,0,0,0,0,0xffff,0xffff)
        directory=struct.pack('<4I',4,0x100,len(metrics),40)+struct.pack('<4I',32,0,len(encoding),40+len(metrics))
        data=b'\x01fcp'+struct.pack('<I',2)+directory+metrics+encoding
        font=SimpleNamespace(glyphs={},unencoded=[{'altcode':None,'meta':['spare',-1,1,1,0,0,1000,0,1,0,None,None,None,None,None,None,['80']]}])
        physical=pcf_physical(data,font)
        self.assertEqual(physical['physical_count'],1)
        self.assertEqual(physical['physical_glyphs'][0]['meta'][0][0],'spare')
        with self.assertRaises(ValueError):pcf_physical(data,SimpleNamespace(glyphs={},unencoded=[]))

    def test_duplicate_notice_records_are_not_lost_in_a_dict(self):
        a=self.profile({'physical_glyphs':[],'raw_property_records':[['COPYRIGHT','"first"'],['COPYRIGHT','"second"']]})
        b=copy.deepcopy(a);b['physical']['raw_property_records'].pop(0)
        self.assertIn('raw_property_records',compare_profiles(a,b))

    def test_x11_final_accelerator_length_rule_does_not_waive_other_truncation(self):
        metrics=struct.pack('<IH',0x100,1)+b'\x80'*5
        encoding=struct.pack('<I6H',0,0,0,0,0,0xffff,0xffff)
        accelerator=struct.pack('<I',0x100)+b'\0'*68
        directory=(struct.pack('<4I',4,0x100,len(metrics),56)
                   +struct.pack('<4I',32,0,len(encoding),56+len(metrics))
                   +struct.pack('<4I',256,0x100,100,56+len(metrics)+len(encoding)))
        data=b'\x01fcp'+struct.pack('<I',3)+directory+metrics+encoding+accelerator
        font=SimpleNamespace(glyphs={},unencoded=[{'altcode':None,'meta':['spare',-1]}])
        result=pcf_physical(data,font)
        self.assertEqual(result['table_directory'][-1]['declared_bytes'],100)
        self.assertEqual(result['table_directory'][-1]['available_bytes'],72)
        with self.assertRaises(ValueError):pcf_physical(data[:-1],font)

    def test_name_platform_raw_bytes_are_inventoried(self):
        def record(raw):return SimpleNamespace(platformID=3,platEncID=1,langID=1033,nameID=0,toUnicode=lambda:'Notice',string=raw)
        self.assertNotEqual(name_records({'name':SimpleNamespace(names=[record(b'a')])}),name_records({'name':SimpleNamespace(names=[record(b'b')])}))


if __name__=='__main__':unittest.main()
