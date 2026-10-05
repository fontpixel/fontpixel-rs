#!/usr/bin/env python3
import struct
import unittest
from phase6_sfnt_validity import checksum,raw_sfnt_validity,SFNT_CHECKSUM,MASK


def fixture(payload=b'abc'):
    head=bytearray(54);head[:4]=b'\0\1\0\0'
    directory=(struct.pack('>4sIII',b'head',checksum(head),44,len(head))
               +struct.pack('>4sIII',b'test',checksum(payload),100,len(payload)))
    data=bytearray(b'\0\1\0\0'+struct.pack('>4H',2,32,1,0)+directory+head+b'\0\0'+payload)
    data+=b'\0'*(-len(data)%4)
    struct.pack_into('>I',data,52,(SFNT_CHECKSUM-checksum(data))&MASK)
    return data


class ChecksumValidityTests(unittest.TestCase):
    def test_two_different_valid_files_have_independently_valid_adjustments(self):
        a=raw_sfnt_validity(fixture(b'abc'));b=raw_sfnt_validity(fixture(b'xyz'))
        self.assertNotEqual(a['stored_checkSumAdjustment'],b['stored_checkSumAdjustment'])
        for proof in [a,b]:self.assertEqual(proof['stored_checkSumAdjustment'],proof['recomputed_checkSumAdjustment'])

    def test_corrupt_table_is_rejected_even_if_whole_checksum_is_repaired(self):
        data=fixture();data[100]^=1
        struct.pack_into('>I',data,52,0)
        struct.pack_into('>I',data,52,(SFNT_CHECKSUM-checksum(data))&MASK)
        with self.assertRaisesRegex(ValueError,'table checksum'):raw_sfnt_validity(data)

    def test_corrupt_adjustment_is_rejected_even_when_table_checksums_match(self):
        data=fixture();data[52]^=1
        with self.assertRaisesRegex(ValueError,'whole sfnt'):raw_sfnt_validity(data)

    def test_duplicate_out_of_bounds_and_web_container_are_rejected(self):
        data=fixture();data[28:32]=b'head'
        with self.assertRaisesRegex(ValueError,'duplicate'):raw_sfnt_validity(data)
        data=fixture();struct.pack_into('>I',data,36,10000)
        with self.assertRaisesRegex(ValueError,'bounds'):raw_sfnt_validity(data)
        data=fixture();data[:4]=b'wOF2'
        with self.assertRaisesRegex(ValueError,'raw standalone'):raw_sfnt_validity(data)


if __name__=='__main__':unittest.main()
