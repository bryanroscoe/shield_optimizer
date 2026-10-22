import copy
from pathlib import Path
import tempfile
import unittest

import catalog
import profiles


class ProfileTests(unittest.TestCase):
    def test_active_ids_only_and_duplicate_collapse(self):
        ids, count = profiles.extract_profile(b'# com.example.comment safe\ncom.example.app # remove safely\ncom.example.app\n\n')
        self.assertEqual(ids, ['com.example.app'])
        self.assertEqual(count, 2)
        for raw in [b'pm disable-user com.example.app', b'../example.bad', b'com.example.app;bad']:
            with self.assertRaises(ValueError):
                profiles.extract_profile(raw)

    def test_pinned_raw_input_is_required(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = catalog.read(profiles.LEADS / 'input-manifest.json')[0]
            p = Path(tmp) / source['path']
            p.parent.mkdir(parents=True)
            p.write_text('com.example.wrong\n')
            with self.assertRaisesRegex(ValueError, 'hash/size mismatch'):
                profiles.extract(Path(tmp))

    def test_committed_replay_and_accounting(self):
        seed = catalog.read(profiles.LEADS / 'seed.json')
        result, report = profiles.build(seed)
        profiles.validate(seed, result)
        self.assertEqual((catalog.DATA / 'profile-candidates.json').read_text(), catalog.encoded(result))
        self.assertEqual((catalog.DATA / 'profile-coverage.json').read_text(), catalog.encoded(report))
        self.assertEqual(report['unique_candidate_ids'], 1190)
        self.assertEqual(report['profiles'], 232)
        self.assertEqual(report['new_ids_beyond_baseline'] + report['baseline_overlap'], 1190)
        self.assertEqual(sum(report['classification_counts'].values()), 1190)
        self.assertEqual(report['text_note_leads'] + report['image_note_leads'], report['device_note_assets'])

    def test_absence_of_independent_metadata_stays_unknown(self):
        seed = catalog.read(profiles.LEADS / 'seed.json')
        result, _ = profiles.build(seed)
        unknown = [r for r in result['packages'] if r['fdroid_ref'] is None]
        self.assertTrue(unknown)
        self.assertTrue(all(r['categories'] == ['unknown'] and r['tv_evidence'] == 'unknown' for r in unknown))
        self.assertTrue(all(r['removal_safety'] == 'unreviewed' and r['installed_on_profile_device'] == 'unverified' for r in result['packages']))
        self.assertTrue(all(p['model'] is None and p['firmware'] is None for p in seed['profiles']))

    def test_schema_rejects_unsupported_claims_and_extra_labels(self):
        seed = catalog.read(profiles.LEADS / 'seed.json')
        result, _ = profiles.build(seed)
        for field, value in [('removal_safety', 'safe'), ('device_compatibility', 'verified'), ('risk', 'safe')]:
            bad = copy.deepcopy(result)
            bad['packages'][0][field] = value
            with self.assertRaises(ValueError):
                profiles.validate(seed, bad)
        bad = copy.deepcopy(seed)
        bad['profiles'][0]['model'] = 'invented verified model'
        with self.assertRaises(ValueError):
            profiles.validate(bad, profiles.build(bad)[0])

    def test_profile_references_and_package_ids_are_unique(self):
        seed = catalog.read(profiles.LEADS / 'seed.json')
        result, _ = profiles.build(seed)
        ids = [r['package'] for r in result['packages']]
        self.assertEqual(ids, sorted(set(ids)))
        for row in result['packages']:
            self.assertEqual(row['profile_refs'], sorted(set(row['profile_refs'])))


if __name__ == '__main__':
    unittest.main()
