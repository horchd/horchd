import navigationPlugin from '@11ty/eleventy-navigation';
import syntaxHighlightingPlugin from '@11ty/eleventy-plugin-syntaxhighlight';
import { eleventyImageTransformPlugin } from '@11ty/eleventy-img';
import markdownIt from 'markdown-it';
import tableOfContentsPlugin from '@uncenter/eleventy-plugin-toc';
import markdownItClass from '@toycode/markdown-it-class';
import markdownItAnchor from 'markdown-it-anchor';
import { library, icon } from '@fortawesome/fontawesome-svg-core';
import { fas } from '@fortawesome/free-solid-svg-icons';
import { execSync } from 'child_process';

export default function (eleventyConfig) {
  eleventyConfig.addPlugin(navigationPlugin);
  eleventyConfig.addPlugin(syntaxHighlightingPlugin);
  eleventyConfig.addPlugin(eleventyImageTransformPlugin, {
    extensions: 'html',
    formats: ['webp', 'jpeg', 'png'],
    widths: ['auto'],
    defaultAttributes: {
      loading: 'lazy',
      decoding: 'async',
    },
  });

  eleventyConfig.addPassthroughCopy('assets');
  eleventyConfig.addPassthroughCopy({ '.domains': '.domains' });

  // Font Awesome Icons
  library.add(fas);
  eleventyConfig.addShortcode('fas_icon', function (args) {
    var fas_svg = icon({ prefix: 'fas', iconName: args });
    return `${fas_svg.html}`;
  });

  const mapping = {
    h2: 'h2 content-title',
    h3: 'h3 content-title',
    h4: 'h4 content-title',
    h5: 'h5 content-title',
    h6: 'h6 content-title',
    table: 'table',
    blockquote: 'alert',
  };

  const mdOptions = { linkify: false, html: true };
  const mdAnchorOpts = {
    permalink: markdownItAnchor.permalink.headerLink(),
    permalinkClass: 'ml-5',
    permalinkSymbol: '#',
    level: [1, 2, 3, 4],
  };

  eleventyConfig.setLibrary(
    'md',
    markdownIt(mdOptions).use(markdownItClass, mapping).use(markdownItAnchor, mdAnchorOpts),
  );

  eleventyConfig.addPairedShortcode('admonition', function (content, type, title) {
    let titleStr = '';
    if (title) {
      titleStr = title;
    } else if (type) {
      titleStr = type.substring(0, 1).toUpperCase() + type.substring(1).toLowerCase();
    } else {
      titleStr = 'Info';
    }
    type = type.toLowerCase();
    return `<div class="alert alert-${type === 'tip' || type === 'note' ? 'primary' : type === 'question' ? 'success' : type}">
      <div class="alert-heading">
        <span class="admonition-icon${type ? ` ${type}` : ''}"></span>
        ${titleStr}</h5>
      </div>
      ${content}
    </div>`;
  });

  eleventyConfig.addPlugin(tableOfContentsPlugin, {
    tags: ['h2', 'h3'],
    wrapper: function (toc) {
      toc = toc.replaceAll('<a', "<a class='simple-link d-block p-1'");
      return `${toc}`;
    },
  });

  // the article list navigation for section index pages
  eleventyConfig.addShortcode('sectionNav', function (collections) {
    const navFilter = eleventyConfig.getFilter('eleventyNavigation');

    // from the nav tree, find the current page's entry
    const entry = navFilter(collections.all).find((page) => page.url == this.page.url);

    // if the current page has children, create a nav table with a link for each
    if (entry.children.length) {
      const rows = entry.children
        .map((child) => `<tr><td><a href="${child.url}">${child.title}</a></td></tr>`)
        .join('');

      return `<table class="table">
        <thead>
          <th>Find out more in this section:</th>
        </thead>
        <tbody>${rows}</tbody>
      </table>`;
    }
  });

  eleventyConfig.on('eleventy.after', () => {
    const runner = process.env.PAGEFIND_RUNNER || 'bunx';
    execSync(`${runner} pagefind`, { encoding: 'utf-8' });
  });
}

export const config = {
  dir: {
    input: 'content',
  },
};
