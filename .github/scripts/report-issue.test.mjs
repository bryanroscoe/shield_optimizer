import { test } from 'node:test';
import assert from 'node:assert/strict';
import { selectReportIssue } from './report-issue.mjs';
const report = {number:273,title:'Device support report',author:{login:'app/github-actions',is_bot:true},body:'<!-- device-support-report -->\nGenerated report'};
test('first report may be created', () => assert.equal(selectReportIssue([]), ''));
test('unrelated issues do not select a target', () => assert.equal(selectReportIssue([{...report,title:'Other issue'}]), ''));
test('generated Actions report selected', () => assert.equal(selectReportIssue([report]), '273'));
test('CRLF marker supported', () => assert.equal(selectReportIssue([{...report,body:'<!-- device-support-report -->\r\nReport'}]), '273'));
for (const [label, change] of Object.entries({human:{author:{login:'bryanroscoe',is_bot:false}},spoofedLogin:{author:{login:'app/github-actions',is_bot:false}},otherBot:{author:{login:'app/dependabot',is_bot:true}},missingMarker:{body:'Hand-written notes'},embeddedMarker:{body:'Notes\n<!-- device-support-report -->'},missingBody:{body:null},missingAuthor:{author:null},invalidNumber:{number:0}})) {
 test(`refuses ${label}`, () => assert.throws(() => selectReportIssue([{...report,...change}])));
}
test('refuses multiple matching reports', () => assert.throws(() => selectReportIssue([report,{...report,number:274}])));
test('refuses malformed list', () => assert.throws(() => selectReportIssue({})));

test('refuses potentially truncated list', () => assert.throws(() => selectReportIssue(Array(500).fill({...report,title:'Other issue'}))));
