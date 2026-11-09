import importlib.util
from pathlib import Path
import socket
import subprocess
import sys
import unittest

spec = importlib.util.spec_from_file_location("ports", Path(__file__).with_name("ports_helper.py"))
ports = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ports)

class PortsTests(unittest.TestCase):
    def test_ipv4_ipv6_and_non_listeners(self):
        header = 'sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n'
        row = '0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 123\n'
        parsed = ports.parse_table(header + row, 'TCP')
        self.assertEqual((parsed[0]['address'], parsed[0]['port'], parsed[0]['inode']), ('127.0.0.1', 3000, '123'))
        self.assertEqual(ports.parse_table(header + row.replace(' 0A ', ' 01 '), 'TCP'), [])
        ipv6 = row.replace('0100007F:0BB8', '00000000000000000000000001000000:0BB8')
        self.assertEqual(ports.parse_table(header + ipv6, 'TCP', True)[0]['address'], '::1')
        self.assertEqual(ports.parse_table(header + 'invalid', 'TCP'), [])

    def test_udp_owned_socket(self):
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.bind(('127.0.0.1', 0))
            row = next(r for r in ports.snapshot()['ports'] if r['port'] == sock.getsockname()[1] and r['protocol'] == 'UDP')
            self.assertTrue(any(o['pid'] == ports.os.getpid() for o in row['owners']))

    def test_live_child_stale_identity_and_graceful_stop(self):
        child = subprocess.Popen([sys.executable, '-u', '-c', "import socket,time;s=socket.socket();s.bind(('127.0.0.1',0));s.listen();print(s.getsockname()[1],flush=True);time.sleep(60)"], stdout=subprocess.PIPE, text=True)
        try:
            port = int(child.stdout.readline())
            row = next(r for r in ports.snapshot()['ports'] if r['port'] == port and r['protocol'] == 'TCP')
            owner = next(o for o in row['owners'] if o['pid'] == child.pid)
            request = dict(owner, inode=row['inode'])
            with self.assertRaises(ValueError):
                ports.stop(dict(request, startedAt='stale'))
            with self.assertRaises(ValueError):
                ports.stop(dict(request, inode='0'))
            self.assertIsNone(child.poll())
            ports.stop(request)
            self.assertEqual(child.wait(timeout=5), -ports.signal.SIGTERM)
        finally:
            if child.poll() is None:
                child.kill(); child.wait()
            child.stdout.close()

    def test_refuses_init_and_self(self):
        for pid in (0, 1, ports.os.getpid()):
            with self.assertRaises(ValueError):
                ports.stop({'pid':pid})

if __name__ == '__main__':
    unittest.main()
