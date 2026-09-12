"""Release shell-boundary regression: python -m unittest discover -s tests -p '*.py'.

Requires PyYAML for parsing the actual workflow, and PowerShell 7.
"""
import os
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]


class ReleaseWorkflow(unittest.TestCase):
    def test_tag_is_data_and_only_release_job_can_write(self):
        workflow = yaml.safe_load((ROOT / '.github/workflows/release.yml').read_text())
        self.assertEqual(workflow['permissions']['contents'], 'read')
        jobs = workflow['jobs']
        self.assertEqual(jobs['create-release']['permissions']['contents'], 'write')
        step = next(s for s in jobs['build-windows-portable']['steps']
                    if s.get('name') == 'Build NSIS installer')
        self.assertEqual(step['env']['TAG'], '${{ inputs.tag || github.ref_name }}')
        for job in jobs.values():
            for item in job['steps']:
                self.assertNotIn('${{', item.get('run', ''))
        # Execute the real version-validation prefix without building an installer.
        prefix = step['run'].split('$exe =', 1)[0]
        self.assertIn('$ver', prefix)
        for tag, valid in [('v1.2.3', True), ('v1.2.3-rc1', False),
                           ('v1.2.3"); Write-Output INJECTED; $ver=("1.2.3', False)]:
            result = subprocess.run(['pwsh', '-NoProfile', '-Command',
                                     "$ErrorActionPreference = 'Stop';\n" + prefix + '\nWrite-Output $ver'],
                                    env={**os.environ, 'TAG': tag}, capture_output=True, text=True)
            self.assertEqual(result.returncode == 0, valid, result.stdout + result.stderr)
            self.assertNotIn('INJECTED', result.stdout)
            if valid:
                self.assertEqual(result.stdout.strip(), '1.2.3')


if __name__ == '__main__':
    unittest.main()
