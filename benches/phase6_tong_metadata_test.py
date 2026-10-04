#!/usr/bin/env python3
"""Small gates for physical Tong source identity and strict metadata verdicts."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import phase6_tong_metadata as check
import phase6_validate as validator


class TongMetadataChecks(unittest.TestCase):
    def bdf(self,glyphs):
        return ('STARTFONT 2.1\nCHARS '+str(len(glyphs))+'\n'+''.join(
            f'STARTCHAR {name}\nENCODING {cp}\nDWIDTH 1 0\nBBX 1 1 0 0\nBITMAP\n{pixels}\nENDCHAR\n'
            for name,cp,pixels in glyphs)+'ENDFONT\n').encode()

    def test_duplicate_scalar_or_unencoded_source_cannot_hide_in_dict(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'input.bdf'
            for records in [[('one',65,'80'),('two',65,'00')],[('unencoded',-1,'80')]]:
                path.write_bytes(self.bdf(records))
                with self.assertRaises(AssertionError):check.source_inventory(path)

    def test_source_physical_bitmap_rows_change_full_record_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'input.bdf'
            path.write_bytes(self.bdf([('one',65,'80')]))
            before=check.source_inventory(path)
            path.write_bytes(self.bdf([('one',65,'00')]))
            after=check.source_inventory(path)
            self.assertNotEqual(before['complete_physical_records_sha256'],after['complete_physical_records_sha256'])

    def test_sfnt_name_notice_metrics_and_unencoded_presence_remain_unresolved(self):
        original={'name_records':[(3,1,1033,0,'notice','00')],
                  'global_tables':{'head':{'lowestRecPPEM':14},'OS/2':{'achVendID':'TONG'}},
                  'all_physical_strikes':[{'all_physical_glyphs':[{'gid':0,'name':'.notdef','hmtx':[0,0],'bitmap_present':False}]}]}
        changed=copy.deepcopy(original)
        changed['name_records']=[]
        changed['global_tables']['head']['lowestRecPPEM']=8
        changed['all_physical_strikes'][0]['all_physical_glyphs'][0]['bitmap_present']=True
        with patch.object(check,'source_inventory',return_value={'physical_count':1}),patch.object(check,'sfnt_inventory',side_effect=[changed,original]):
            result=check.bitmap_metadata('source','native','reference','otb','work')
        self.assertEqual(set(result['unresolved_differences']),set(original))
        self.assertFalse(result['differences_accepted'])

    def test_metadata_failure_cannot_skip_pixels_or_produce_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);paths=[root/name for name in ['source.bdf','native.otb','reference.otb']]
            for path in paths:path.write_bytes(b'identity fixture')
            with patch.object(check,'bitmap_metadata',side_effect=ValueError('bad name record')),patch.object(validator,'bitmap_pair',return_value={'all_pixels':True}) as pixels:
                result=validator.bitmap_pair_proof(*paths,'otb',root,root)
            pixels.assert_called_once()
            self.assertEqual(result['status'],'fail');self.assertEqual(result['failed_gates'],['metadata'])
            self.assertTrue(Path(result['detail_file']).is_file())

    def test_unresolved_metadata_cannot_be_accepted_after_pixels_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);paths=[root/name for name in ['source.bdf','native.otb','reference.otb']]
            for path in paths:path.write_bytes(b'identity fixture')
            with patch.object(check,'bitmap_metadata',return_value={'unresolved_differences':{'name_records':{'native':[],'reference':['OFL']}}}),patch.object(validator,'bitmap_pair',return_value={'all_pixels':True}):
                result=validator.bitmap_pair_proof(*paths,'otb',root,root)
            self.assertEqual(result['status'],'fail');self.assertEqual(result['failed_gates'],[])
            self.assertEqual(result['unresolved_metadata_fields'],['name_records'])


if __name__=='__main__':unittest.main()
