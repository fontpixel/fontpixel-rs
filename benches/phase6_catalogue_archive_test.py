#!/usr/bin/env python3
import unittest
from phase6_catalogue_validate import archive_member_dependencies


class ArchiveDependencyTests(unittest.TestCase):
    def tasks(self):
        return [{'slug':'family','kind':'bdf','file':'family--a.bdf.gz'},
                {'slug':'family','kind':'ttf','file':'family.ttf'},
                {'slug':'other','kind':'bdf','file':'other--b.bdf.gz'}]

    def record(self,name,status):
        return {'file':name,'status':status,'proof':'proof/'+name,'proof_sha256':'identity'}

    def test_unresolved_bdf_member_blocks_semantic_archive_success(self):
        fonts=[self.record('family--a.bdf.gz','unresolved-differences')]
        result=archive_member_dependencies('family',self.tasks(),fonts)
        self.assertEqual(len(result),1);self.assertEqual(result[0]['status'],'unresolved-differences')

    def test_other_family_and_unrelated_vector_do_not_mask_member_result(self):
        fonts=[self.record('family--a.bdf.gz','pass'),self.record('family.ttf','fail'),self.record('other--b.bdf.gz','fail')]
        self.assertEqual(archive_member_dependencies('family',self.tasks(),fonts),[])

    def test_missing_font_proof_is_not_a_packaging_pass(self):
        with self.assertRaises(AssertionError):archive_member_dependencies('family',self.tasks(),[])


if __name__=='__main__':unittest.main()
