'use strict';

const CASE_ID = window.CASE_ID;
const LOOK = new URLSearchParams(location.search).get('look') || 'pencil';

function strokePath(c, pts, close = true, width = 7, color = '#302c29') {
  c.beginPath();
  pts.forEach(([x, y], i) => i ? c.lineTo(x, y) : c.moveTo(x, y));
  if (close) c.closePath();
  c.lineWidth = width; c.lineJoin = 'round'; c.lineCap = 'round';
  c.strokeStyle = color; c.stroke();
}
function fillPath(c, pts, fill, width = 7) {
  c.beginPath(); pts.forEach(([x,y],i)=>i?c.lineTo(x,y):c.moveTo(x,y)); c.closePath();
  c.fillStyle = fill; c.fill(); strokePath(c, pts, true, width);
}
function ellipse(c, x, y, rx, ry, fill, width = 7) {
  c.beginPath(); c.ellipse(x,y,rx,ry,0,0,Math.PI*2); c.fillStyle=fill; c.fill();
  c.lineWidth=width; c.strokeStyle='#302c29'; c.stroke();
}
function line(c, a, b, width=6, color='#302c29') { c.beginPath(); c.moveTo(...a); c.lineTo(...b); c.lineWidth=width; c.lineCap='round'; c.strokeStyle=color; c.stroke(); }
function eye(c,x,y) { ellipse(c,x,y,18,21,'#fff',5); ellipse(c,x,y,6,8,'#302c29',2); }
function hatch(c, x, y, w, h, color='#6d6257') { c.save(); c.beginPath(); c.rect(x,y,w,h); c.clip(); c.strokeStyle=color; c.globalAlpha=.25; c.lineWidth=3; for(let i=-h;i<w+h;i+=18) line(c,[x+i,y+h],[x+i+h,y],3,color); c.restore(); }

function cat(c, x, y, s=1, sleepy=false) {
  c.save(); c.translate(x,y); c.scale(s,s);
  fillPath(c,[[-150,120],[-165,40],[-135,-35],[-70,-75],[30,-72],[115,-35],[150,45],[130,120],[55,150],[-55,150]], '#d9cfc2');
  fillPath(c,[[-90,-70],[-92,-190],[-28,-138],[25,-154],[90,-198],[75,-65],[25,-20],[-35,-18]], '#e6cfc9');
  line(c,[-34,5],[-44,88],9); line(c,[38,5],[52,88],9);
  line(c,[-82,112],[-110,160],10); line(c,[74,112],[106,160],10);
  fillPath(c,[[108,50],[170,10],[205,35],[186,70],[130,84]], '#d9cfc2',6);
  if (sleepy) { line(c,[-55,-88],[-25,-82],6); line(c,[32,-82],[60,-88],6); }
  else { eye(c,-48,-90); eye(c,42,-90); }
  fillPath(c,[[-17,-42],[17,-42],[25,-20],[0,-7],[-25,-20]], '#c88982',4);
  c.restore();
}
function dog(c,x,y,s=1) {
  c.save(); c.translate(x,y); c.scale(s,s);
  fillPath(c,[[-175,55],[-155,-35],[-70,-75],[35,-70],[140,-25],[168,42],[125,85],[20,105],[-90,100]], '#c6b7a4');
  fillPath(c,[[-90,-62],[-105,-165],[-35,-126],[40,-142],[102,-176],[83,-60],[35,-15],[-35,-15]], '#d8c3b8');
  fillPath(c,[[-112,-95],[-185,-40],[-178,35],[-126,5]], '#b39a8b',5); fillPath(c,[[105,-108],[178,-35],[164,35],[118,5]], '#b39a8b',5);
  line(c,[-105,65],[-125,150],12); line(c,[92,65],[126,145],12); line(c,[-30,78],[-25,150],12); line(c,[35,78],[48,150],12);
  fillPath(c,[[137,25],[210,-18],[236,10],[207,42],[148,55]], '#c6b7a4',6);
  eye(c,-42,-92); eye(c,38,-92); ellipse(c,0,-43,38,27,'#c88982',5); c.restore();
}
function person(c,x,y,s=1,pipe=false) {
  c.save(); c.translate(x,y); c.scale(s,s);
  fillPath(c,[[-105,125],[-125,20],[-88,-40],[15,-45],[78,20],[90,130]], '#b9a98e');
  fillPath(c,[[-88,-45],[-110,-150],[-70,-205],[10,-218],[70,-165],[62,-58],[10,-25],[-50,-20]], '#e7c5bd');
  fillPath(c,[[-116,-140],[-94,-218],[-25,-248],[58,-230],[82,-175],[-15,-170]], '#8e877d',6);
  line(c,[-62,120],[-82,250],14); line(c,[40,120],[67,250],14);
  line(c,[22,-10],[145,45],14); line(c,[145,45],[226,10],8);
  eye(c,18,-142); line(c,[38,-102],[66,-98],6);
  if (pipe) { line(c,[67,-91],[168,-58],7); ellipse(c,178,-50,22,28,'#806b5a',5); }
  c.restore();
}
function elephant(c,x,y,s=1) {
  c.save(); c.translate(x,y); c.scale(s,s);
  fillPath(c,[[-180,90],[-190,-5],[-135,-85],[-25,-110],[95,-80],[165,0],[145,95],[45,135],[-80,130]], '#aeb5ad');
  fillPath(c,[[-100,-52],[-135,-150],[-105,-225],[-40,-200],[10,-135],[72,-205],[130,-216],[115,-80],[45,-38]], '#bfc5bd');
  ellipse(c,-51,-105,18,23,'#fff',4); ellipse(c,45,-105,18,23,'#fff',4);
  line(c,[-10,-65],[-8,10],[10][0]);
  c.beginPath(); c.moveTo(-8,-65); c.bezierCurveTo(-25,10,-10,90,45,110); c.lineWidth=18; c.strokeStyle='#9fa79f'; c.stroke();
  line(c,[-105,75],[-110,175],14); line(c,[65,78],[75,180],14); line(c,[-20,95],[-20,180],14);
  fillPath(c,[[90,-5],[210,20],[215,48],[95,35]], '#e4d9c6',5); c.restore();
}
function bike(c,x,y,s=1) {
  c.save(); c.translate(x,y); c.scale(s,s); person(c,0,-30,1,false);
  c.lineWidth=8; c.strokeStyle='#302c29'; c.beginPath(); c.arc(-100,160,72,0,Math.PI*2); c.stroke(); c.beginPath(); c.arc(145,160,72,0,Math.PI*2); c.stroke();
  line(c,[-100,160],[10,65],7); line(c,[10,65],[145,160],7); line(c,[-100,160],[145,160],7); line(c,[10,65],[-15,160],7); line(c,[10,65],[70,25],7); line(c,[60,20],[105,20],7);
  c.restore();
}
function house(c,x,y,s=1) { c.save(); c.translate(x,y); c.scale(s,s); fillPath(c,[[-180,100],[-180,-40],[0,-180],[180,-40],[180,100]],'#d9c7aa'); fillPath(c,[[-210,-35],[0,-205],[210,-35]],'#a48d77'); fillPath(c,[[-45,100],[-45,10],[45,10],[45,100]],'#a98b72',5); c.restore(); }

function shot(c) {
  paper(c, LOOK === 'ink' ? '#f2eadc' : '#f5f0e5', null, 85);
  if (CASE_ID === 'cat-cushion') { ellipse(c,540,710,360,100,'#e9ded0',6); cat(c,540,510,1.05,true); }
  if (CASE_ID === 'dog-run') dog(c,540,520,1.12);
  if (CASE_ID === 'pipe-garden') { person(c,540,505,.95,true); ellipse(c,540,780,300,30,'#b1ae7b',4); line(c,[170,700],[170,260],14,'#7b7055'); ellipse(c,170,230,90,100,'#aaa97a',5); line(c,[900,700],[900,230],14,'#7b7055'); ellipse(c,900,190,105,120,'#aaa97a',5); }
  if (CASE_ID === 'elephant-reading') { elephant(c,540,540,1.0); }
  if (CASE_ID === 'bike-rider') bike(c,540,420,.72);
  if (CASE_ID === 'two-subjects') { cat(c,310,530,.63); dog(c,770,530,.63); house(c,540,610,.65); }
}

defineFilm({palette:{...PALETTES.pencilMinimal,paper:'#f5f0e5'},format:{ar:'1:1',width:1024},fps:24,timeline:[{name:CASE_ID,dur:1.0,fn:shot}]});
