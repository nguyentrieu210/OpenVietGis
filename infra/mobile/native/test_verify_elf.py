import struct
import unittest
from verify_elf import inspect

def elf(align=16384, machine=183, offset=0, va=0):
    e = bytearray(128)
    struct.pack_into("<16sHHIQQQIHHHHHH",e,0,b"\x7fELF\x02\x01\x01",3,machine,1,0,64,0,0,64,56,1,0,0,0)
    struct.pack_into("<IIQQQQQQ",e,64,1,5,offset,va,0,128,128,align)
    return bytes(e)
class TestElf(unittest.TestCase):
    def test_valid(self):
        self.assertEqual(inspect(elf())[0]["alignment"],16384)
    def test_4k_rejected(self):
        with self.assertRaises(ValueError): inspect(elf(4096))
    def test_wrong_abi_rejected(self):
        with self.assertRaises(ValueError): inspect(elf(machine=62))
    def test_offset_rejected(self):
        with self.assertRaises(ValueError): inspect(elf(va=4096))
    def test_truncated_rejected(self):
        with self.assertRaises(ValueError): inspect(elf()[:75])
if __name__ == "__main__": unittest.main()
