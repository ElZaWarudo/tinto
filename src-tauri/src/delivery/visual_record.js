document.querySelectorAll('.panel').forEach(p=>{
  const tabs=[...p.querySelectorAll('.tabs button')],shots=[...p.querySelectorAll('.shot')],frame=p.querySelector('.browser,.phone');
  tabs.forEach((t,i)=>t.addEventListener('click',()=>{
    // Hold the frame's height while the next image decodes, so the page doesn't collapse and jump.
    frame.style.minHeight=frame.offsetHeight+'px';
    tabs.forEach((x,j)=>x.setAttribute('aria-selected',i===j));shots.forEach((s,j)=>s.classList.toggle('on',i===j));
    const im=shots[i].querySelector('img');im.loading='eager';
    (im.complete?Promise.resolve():new Promise(r=>{im.onload=im.onerror=r})).then(()=>{
      frame.style.minHeight='';
      // A shorter image can leave the panel above the screen; bring it back.
      if(p.getBoundingClientRect().top<0)p.scrollIntoView({block:'start'});});}));
});
const imgs=[...document.querySelectorAll('.shot img')],lb=document.querySelector('.lightbox'),
  lbImg=lb.querySelector('img'),lbCap=lb.querySelector('.cap');let cur=0;
function show(i){cur=(i+imgs.length)%imgs.length;const im=imgs[cur];lbImg.src=im.src;
  lbCap.textContent=(cur+1)+' / '+imgs.length+' · '+im.dataset.section+' · '+im.alt;lb.classList.add('on');}
imgs.forEach((im,i)=>im.addEventListener('click',()=>show(i)));
const base=s=>s.trim().split(/[\\/]/).pop().toLowerCase(),byFile=new Map(imgs.map((im,i)=>[base(im.dataset.file||''),i]));
document.querySelectorAll('main code').forEach(c=>{const i=byFile.get(base(c.textContent));if(i===undefined)return;
  c.classList.add('shotref');c.tabIndex=0;c.setAttribute('role','button');c.title='Open screenshot';
  c.addEventListener('click',()=>show(i));
  c.addEventListener('keydown',e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();show(i);}});});
lb.querySelector('.prev').onclick=e=>{e.stopPropagation();show(cur-1)};
lb.querySelector('.next').onclick=e=>{e.stopPropagation();show(cur+1)};
lb.querySelector('.close').onclick=()=>lb.classList.remove('on');
lbImg.onclick=()=>lb.classList.remove('on');
lb.addEventListener('click',e=>{if(e.target===lb)lb.classList.remove('on')});
document.addEventListener('keydown',e=>{if(!lb.classList.contains('on'))return;
  if(e.key==='Escape')lb.classList.remove('on');if(e.key==='ArrowLeft')show(cur-1);if(e.key==='ArrowRight')show(cur+1);});
const links=[...document.querySelectorAll('nav a')];
const io=new IntersectionObserver(es=>es.forEach(e=>{if(e.isIntersecting)
  links.forEach(a=>a.classList.toggle('on',a.getAttribute('href')==='#'+e.target.id));}),{rootMargin:'-20% 0px -70% 0px'});
document.querySelectorAll('section.change').forEach(s=>io.observe(s));
