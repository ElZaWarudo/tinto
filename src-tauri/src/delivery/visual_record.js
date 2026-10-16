document.querySelectorAll('.panel').forEach(p=>{
  const tabs=[...p.querySelectorAll('.tabs button')],shots=[...p.querySelectorAll('.shot')];
  tabs.forEach((t,i)=>t.addEventListener('click',()=>{
    tabs.forEach((x,j)=>x.setAttribute('aria-selected',i===j));shots.forEach((s,j)=>s.classList.toggle('on',i===j));}));
});
const imgs=[...document.querySelectorAll('.shot img')],lb=document.querySelector('.lightbox'),
  lbImg=lb.querySelector('img'),lbCap=lb.querySelector('.cap');let cur=0;
function show(i){cur=(i+imgs.length)%imgs.length;const im=imgs[cur];lbImg.src=im.src;
  lbCap.textContent=(cur+1)+' / '+imgs.length+' · '+im.dataset.section+' · '+im.alt;lb.classList.add('on');}
imgs.forEach((im,i)=>im.addEventListener('click',()=>show(i)));
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
