"""Regression checks for the pinned sync-release-gitee action; no live API writes."""
import contextlib
import importlib
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, MagicMock, patch

ACTION = Path(os.environ['SYNC_ACTION_PATH']).resolve()
sys.path.insert(0, str(ACTION))
os.environ.update(gitee_token='TEST_ONLY_TOKEN_abcdef', debug='false')
sync = importlib.import_module('sync_releases')
client = importlib.import_module('gitee_release')


def response(payload, status=200):
    result = MagicMock(status_code=status)
    result.__enter__.return_value = result
    result.json.return_value = payload
    result.raise_for_status.side_effect = None
    return result


class HardeningTests(unittest.TestCase):
    values = dict(gitee_owner='karmakarma', gitee_repo='Karma', github_owner='csbuddhist', github_repo='Karma')

    def test_false_debug_does_not_enable_debug_logging(self):
        self.assertIs(sync.debug_mode, False)

    def test_public_github_get_keeps_tls_verification(self):
        with patch.object(sync.requests, 'get', return_value=response([])) as get:
            sync.fetch_github_releases('csbuddhist', 'Karma')
        self.assertTrue(get.call_args.kwargs.get('verify', True))
        self.assertIsNotNone(get.call_args.kwargs.get('timeout'))

    def test_releases_beyond_first_page_are_included(self):
        first = [{'id': n, 'tag_name': f'v{n}'} for n in range(100)]
        with patch.object(sync.requests, 'get', side_effect=[response(first), response([{'id': 100, 'tag_name': 'v100'}])]):
            releases, _ = sync.fetch_github_releases('csbuddhist', 'Karma')
        self.assertEqual(len(releases), 101)
        self.assertEqual(releases[-1]['tag_name'], 'v100')

    def test_api_error_is_failure_without_echoing_token(self):
        with patch.object(sync.requests, 'get', return_value=response({'message': 'TEST_ONLY_TOKEN_abcdef'}, 403)):
            with self.assertRaises(RuntimeError) as error:
                sync.fetch_github_releases('csbuddhist', 'Karma')
        self.assertNotIn('TEST_ONLY_TOKEN', str(error.exception))

    def test_network_error_is_sanitized(self):
        with patch.object(sync.requests, 'get', side_effect=sync.requests.ConnectionError('https://gitee.com/?access_token=TEST_ONLY_TOKEN_abcdef')):
            with self.assertRaises(RuntimeError) as error:
                sync.fetch_gitee_releases('karmakarma', 'Karma')
        self.assertNotIn('TEST_ONLY_TOKEN', str(error.exception))

    def test_false_debug_never_prints_a_token_prefix(self):
        values = dict(gitee_owner='karmakarma', gitee_repo='Karma', github_owner='csbuddhist', github_repo='Karma')
        output = io.StringIO()
        with patch.dict(os.environ, values), patch.object(sync, 'fetch_github_releases', return_value=([], 'github')), patch.object(sync, 'fetch_gitee_releases', return_value=({}, 'gitee')), contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            sync.sync_github_releases_to_gitee()
        self.assertNotIn('TEST_ONLY', output.getvalue())

    def test_creation_preserves_prerelease(self):
        with patch.object(client.requests, 'post', return_value=response({'id': 5})) as post:
            ok, identifier = client.Gitee('karmakarma', 'TEST_ONLY_TOKEN_abcdef').create_release('Karma', 'v1', 'Test', prerelease=True)
        self.assertTrue(ok)
        self.assertEqual(identifier, 5)
        self.assertEqual(post.call_args.kwargs['data']['prerelease'], 'true')
        self.assertTrue(post.call_args.kwargs.get('verify', True))

    def test_creation_api_failure_is_reported_without_response_body(self):
        with patch.object(client.requests, 'post', return_value=response({'message': 'TEST_ONLY_TOKEN_abcdef'}, 422)):
            ok, message = client.Gitee('karmakarma', 'TEST_ONLY_TOKEN_abcdef').create_release('Karma', 'v1', 'Test')
        self.assertFalse(ok)
        self.assertNotIn('TEST_ONLY_TOKEN', message)

    def test_download_failure_fails_the_run(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(sync.requests, 'get', return_value=response({}, 404)):
            with self.assertRaises(RuntimeError):
                sync.download_file_from_url('https://github.com/x/y/releases/download/v1/a.exe', folder, 'a.exe')

    def test_download_cannot_escape_its_directory(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(sync.requests, 'get') as get:
            with self.assertRaises(ValueError):
                sync.download_file_from_url('https://github.com/x/y', folder, '../escape.exe')
        get.assert_not_called()

    def test_same_named_asset_is_not_uploaded_again(self):
        asset = {'name': 'setup.exe', 'browser_download_url': 'https://github.com/x/y/releases/download/v1/setup.exe'}
        gitee = client.Gitee('karmakarma', 'TEST_ONLY_TOKEN_abcdef')
        with patch.object(sync, 'fetch_gitee_release_assets', return_value={'setup.exe': {'name': 'setup.exe'}}), patch.object(sync, 'download_file_from_url') as download:
            sync.sync_release_assets_only(gitee, {'setup.exe': asset}, 'v1', {'id': 3}, 'Karma')
        download.assert_not_called()

    def test_upload_http_error_is_a_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'setup.exe'
            path.write_bytes(b'not an executable; fixture only')
            gitee = client.Gitee('karmakarma', 'TEST_ONLY_TOKEN_abcdef')
            with patch.object(client.requests, 'post', return_value=response({'message': 'TEST_ONLY_TOKEN_abcdef'}, 500)):
                with self.assertRaises(RuntimeError) as error:
                    gitee.upload_asset('Karma', 3, file_name='setup.exe', file_path=str(path))
        self.assertNotIn('TEST_ONLY_TOKEN', str(error.exception))

    def test_full_run_preserves_prerelease_and_uploads_bytes(self):
        release = {'id': 1, 'tag_name': 'v1', 'name': 'Test', 'body': 'English\n中文', 'target_commitish': 'main', 'prerelease': True, 'draft': False, 'created_at': '2026-10-25T00:00:00Z'}
        asset = {'id': 2, 'name': 'setup.exe', 'size': 3, 'browser_download_url': 'https://github.com/csbuddhist/Karma/releases/download/v1/setup.exe'}
        destinations = iter([[], [{'id': 5, 'tag_name': 'v1', 'prerelease': True}]])

        def get(url, **kwargs):
            if url.endswith('/tags'):
                return response([{'name': 'v1'}])
            if url.startswith('https://gitee.com/') and url.endswith('/releases'):
                return response(next(destinations))
            if url.endswith('/attach_files'):
                return response([])
            if url.endswith('/releases/1/assets'):
                return response([asset])
            if url.endswith('/releases/1'):
                return response(release)
            if url.endswith('/releases'):
                return response([release])
            if url.endswith('/setup.exe'):
                result = response({})
                result.iter_content.return_value = [b'abc']
                return result
            raise AssertionError('Unexpected URL')

        uploaded = []
        def post(url, **kwargs):
            if url.endswith('/attach_files'):
                uploaded.append(b''.join(iter(lambda: kwargs['data'].read(65536), b'')))
                return response({'browser_download_url': 'https://gitee.com/download/setup.exe'})
            self.assertEqual(kwargs['data']['prerelease'], 'true')
            self.assertEqual(kwargs['data']['body'], 'English\n中文')
            return response({'id': 5})

        with patch.dict(os.environ, self.values), patch.object(sync.requests, 'get', side_effect=get), patch.object(client.requests, 'post', side_effect=post), patch.object(sync, 'set_action_output'):
            sync.sync_github_releases_to_gitee()
        self.assertEqual(len(uploaded), 1)
        self.assertIn(b'abc', uploaded[0])

    def test_drafts_never_create_gitee_releases(self):
        with patch.dict(os.environ, self.values), patch.object(sync, 'fetch_github_releases', return_value=([{'draft': True}], 'github')), patch.object(sync, 'fetch_gitee_releases', return_value=({}, 'gitee')), patch.object(client.requests, 'post') as post:
            sync.sync_github_releases_to_gitee()
        post.assert_not_called()

    def test_unmirrored_tag_fails_before_any_release_is_created(self):
        with patch.dict(os.environ, self.values), patch.object(sync, 'fetch_github_releases', return_value=([{'draft': False, 'tag_name': 'v2'}], 'github')), patch.object(sync, 'fetch_gitee_releases', return_value=({}, 'gitee')), patch.object(sync, 'fetch_pages', return_value=[]), patch.object(client.requests, 'post') as post:
            with self.assertRaisesRegex(RuntimeError, 'not fully mirrored'):
                sync.sync_github_releases_to_gitee()
        post.assert_not_called()

    def test_release_creation_failure_fails_the_workflow(self):
        with patch.object(client.requests, 'post', return_value=response({}, 422)):
            with self.assertRaises(RuntimeError):
                sync.create_gitee_release('karmakarma', 'TEST_ONLY_TOKEN_abcdef', 'Karma', 'v1', 'Test', '-', 'main')

    def test_prerelease_promotion_updates_the_existing_release(self):
        release = {'id': 1, 'tag_name': 'v1', 'name': 'Test', 'target_commitish': 'main', 'prerelease': False}
        current = {'id': 5, 'tag_name': 'v1', 'prerelease': True}
        with patch.dict(os.environ, self.values), patch.object(sync, 'fetch_github_releases', return_value=([release], 'github')), patch.object(sync, 'fetch_gitee_releases', return_value=({'v1': current}, 'gitee')), patch.object(sync, 'fetch_pages', return_value=[{'name': 'v1'}]), patch.object(sync, 'fetch_github_release_details', return_value=(release, {}, 'github')), patch.object(sync, 'sync_release_assets_only'), patch.object(client.requests, 'patch', return_value=response(current)) as update:
            sync.sync_github_releases_to_gitee()
        self.assertEqual(update.call_args.kwargs['data']['prerelease'], 'false')

    def test_only_latest_published_release_uploads_assets(self):
        releases = [
            {'id': 1, 'tag_name': 'v1', 'published_at': '2026-10-24T00:00:00Z', 'created_at': '2026-10-25T00:00:00Z'},
            {'id': 2, 'tag_name': 'v2', 'published_at': '2026-10-25T00:00:00Z', 'created_at': '2026-10-23T00:00:00Z'},
        ]
        current = {'v1': {'id': 11}, 'v2': {'id': 12}}
        def details(owner, repo, identifier):
            return next(r for r in releases if r['id'] == identifier), {}, 'github'
        with patch.dict(os.environ, self.values), patch.object(sync, 'fetch_github_releases', return_value=(releases, 'github')), patch.object(sync, 'fetch_gitee_releases', return_value=(current, 'gitee')), patch.object(sync, 'fetch_pages', return_value=[{'name': 'v1'}, {'name': 'v2'}]), patch.object(sync, 'fetch_github_release_details', side_effect=details), patch.object(sync, 'sync_release_assets_only') as upload:
            sync.sync_github_releases_to_gitee()
        self.assertEqual([call.args[2] for call in upload.call_args_list], ['v2'])


if __name__ == '__main__':
    unittest.main()
