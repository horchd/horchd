if (localStorage.getItem('theme') === 'light') {
  document.documentElement.setAttribute('data-bs-theme', 'light');
}

function toggleDarkMode() {
  const rootPreference = document.documentElement.getAttribute('data-bs-theme');
  if (rootPreference === 'light') {
    document.documentElement.setAttribute('data-bs-theme', 'dark');
    localStorage.setItem('theme', 'dark');
  } else {
    document.documentElement.setAttribute('data-bs-theme', 'light');
    localStorage.setItem('theme', 'light');
  }
}

function toggleSidebar() {
  const sidebarEl = document.querySelector('.sidebar');
  if (sidebarEl.classList.contains('show')) {
    sidebarEl.classList.remove('show');
    sidebarEl.classList.add('hiding');
  } else {
    sidebarEl.classList.remove('hiding');
    sidebarEl.classList.add('show');
  }
}
