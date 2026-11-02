import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function selectReportIssue(issues) {
  if (!Array.isArray(issues)) throw new Error('Expected an issue list');
  if (issues.length >= 500) throw new Error('Issue list may be truncated; refusing to choose a target');
  const matches = issues.filter(issue => issue.title === 'Device support report');
  if (matches.length > 1) throw new Error('Multiple report issues; refusing an ambiguous update');
  if (!matches.length) return '';
  const issue = matches[0];
  if (issue.author?.is_bot !== true || issue.author.login !== 'app/github-actions' ||
      typeof issue.body !== 'string' ||
      issue.body.split(/\r?\n/, 1)[0] !== '<!-- device-support-report -->' ||
      !Number.isSafeInteger(issue.number) || issue.number < 1) {
    throw new Error('Refusing to overwrite an issue without the report marker and Actions bot identity');
  }
  return String(issue.number);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  console.log(selectReportIssue(JSON.parse(readFileSync(0, 'utf8'))));
}
